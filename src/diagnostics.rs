//! Diagnostics: the structured `Diagnostic` the checker emits and the conformance
//! harness consumes, plus terminal rendering via `codespan-reporting`.
//!
//! The harness consumes *structured* diagnostics (code + message + span), not
//! rendered text (mvp-plan §2/§3), so the data model is the source of truth and
//! the terminal rendering is a separate, presentation-only step.

use crate::span::Span;
use crate::types::repr::TypeTag;
use crate::types::store::{Store, TypeId};
use codespan_reporting::diagnostic::{Diagnostic as CsDiagnostic, Label};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};

/// A tsc-compatible diagnostic code (numbers reused from tsc, `TK` prefix to be
/// honest about source — mvp-plan §2). Only the codes the MVP can emit are
/// listed; later milestones add variants.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DiagnosticCode {
    /// Cannot find name (unresolved identifier).
    TK2304,
    /// Type X is not assignable to type Y.
    TK2322,
    /// Property does not exist on type (member access).
    TK2339,
    /// Argument type is not assignable to the parameter type (call argument).
    TK2345,
    /// Object literal may only specify known properties (excess property).
    TK2353,
    /// Wrong number of call arguments (arity).
    TK2554,
    /// Property is missing in type but required.
    TK2741,
}

impl DiagnosticCode {
    /// The rendered code string, e.g. `"TK2322"`.
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::TK2304 => "TK2304",
            DiagnosticCode::TK2322 => "TK2322",
            DiagnosticCode::TK2339 => "TK2339",
            DiagnosticCode::TK2345 => "TK2345",
            DiagnosticCode::TK2353 => "TK2353",
            DiagnosticCode::TK2554 => "TK2554",
            DiagnosticCode::TK2741 => "TK2741",
        }
    }
}

/// Severity of a diagnostic. M0 only emits errors; the enum exists so warnings
/// (e.g. future lint-style notices) slot in without changing the data model.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    #[allow(dead_code)] // TODO: reserved for non-error diagnostics.
    Warning,
}

/// A structured diagnostic. `span` is the **primary** span — its start is what
/// the harness maps to a 1-based line for marker matching.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    /// Construct a `TK2322` "not assignable" error with the given primary span
    /// and fully-rendered message.
    pub fn not_assignable(span: Span, message: String) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2322,
            severity: Severity::Error,
            message,
            span,
        }
    }

    /// Construct a `TK2304` "cannot find name" error. The primary span is the
    /// unresolved identifier reference.
    pub fn cannot_find_name(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2304,
            severity: Severity::Error,
            message: format!("Cannot find name '{name}'"),
            span,
        }
    }

    /// Construct a `TK2339` "property does not exist" error (member access of an
    /// unknown property). The primary span is the property name. `tgt` is the
    /// rendered object type the property was looked up on.
    pub fn property_does_not_exist(span: Span, name: &str, tgt: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2339,
            severity: Severity::Error,
            message: format!("Property '{name}' does not exist on type '{tgt}'"),
            span,
        }
    }

    /// Construct a `TK2741` "property is missing" error: a fresh value/literal is
    /// assigned to an object annotation that requires a property the source
    /// lacks. The primary span is the source literal/expression. `tgt` is the
    /// rendered target object type.
    pub fn property_missing(span: Span, name: &str, tgt: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2741,
            severity: Severity::Error,
            message: format!("Property '{name}' is missing in type '{tgt}'"),
            span,
        }
    }

    /// Construct a `TK2353` excess-property error: a fresh object literal
    /// specifies a property `name` not present in the object-typed target `tgt`.
    /// The primary span is the offending property.
    pub fn excess_property(span: Span, name: &str, tgt: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2353,
            severity: Severity::Error,
            message: format!("'{name}' does not exist in type '{tgt}'"),
            span,
        }
    }

    /// Construct a `TK2345` "argument not assignable" error: a call argument of
    /// type `src` is not assignable to the parameter type `tgt`. The primary span
    /// is the offending argument.
    pub fn argument_not_assignable(span: Span, src: &str, tgt: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2345,
            severity: Severity::Error,
            message: format!(
                "Argument of type '{src}' is not assignable to parameter of type '{tgt}'"
            ),
            span,
        }
    }

    /// Construct a `TK2554` arity error: a call passed `got` arguments but the
    /// callee expects `expected`. The primary span is the call expression.
    pub fn wrong_argument_count(span: Span, expected: usize, got: usize) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2554,
            severity: Severity::Error,
            message: format!("Expected {expected} arguments, but got {got}"),
            span,
        }
    }

    /// Whether this diagnostic counts as an error (drives the process exit code).
    pub fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }

    /// The fully-rendered diagnostic text the harness substring-matches against
    /// (`tests/cases/README.md`: case-sensitive `contains` over the rendered
    /// text, "including any nested reason chain"). For M0 this is just
    /// `code + message`; M6 appends the nested reason chain here.
    pub fn rendered_text(&self) -> String {
        format!("error[{}]: {}", self.code.as_str(), self.message)
    }
}

/// Render the display name of a type (the "Type display format" of
/// `tests/cases/README.md`).
///
/// `widen`: when `true`, a literal type renders as its base intrinsic
/// (`"hello"` → `string`, `42` → `number`, `true` → `boolean`). This is how the
/// **source** side of an assignability message is shown — assignability *logic*
/// uses the literal type, but the *message* widens it (mvp-plan M0 spec). The
/// target side renders with `widen = false`.
///
/// Rendering is **cycle-safe** (M5): a recursive named type (`interface List {
/// tail: List | null }`) would otherwise expand forever. An object type already
/// being rendered higher in the stack is printed as `...` instead of re-expanded.
/// Object/named-type targets are asserted code-only in the corpus, so the exact
/// placeholder text is unconstrained — it only has to terminate.
pub fn render_type(store: &Store, id: TypeId, widen: bool) -> String {
    let mut rendering: Vec<TypeId> = Vec::new();
    render_type_inner(store, id, widen, &mut rendering)
}

/// Cycle-safe core of [`render_type`]. `rendering` holds the object ids currently
/// being expanded on the call stack; re-entering one emits `...` to break the
/// cycle. Only object types can be self-referential (interfaces are the only
/// nominal types), so the guard is keyed on object ids — but it is threaded
/// through every recursive arm so a cycle reached *via* a union/function member is
/// also broken.
fn render_type_inner(store: &Store, id: TypeId, widen: bool, rendering: &mut Vec<TypeId>) -> String {
    match store.tag(id) {
        TypeTag::Intrinsic => store
            .intrinsic_kind(id)
            .map(|k| k.display_name().to_string())
            // Defensive fallback; an intrinsic always has a kind.
            .unwrap_or_else(|| "unknown".to_string()),
        TypeTag::Literal => {
            let value = store.literal_value(id);
            match value {
                Some(lit) if widen => lit.base_kind().display_name().to_string(),
                Some(lit) => render_literal(lit),
                // Defensive fallback; a literal always has a value.
                None => "unknown".to_string(),
            }
        }
        // Object: `{ a: number; b: string }` — members in stored (canonical)
        // order, `; `-separated (README "Type display format"). Property *types*
        // never widen (they are already the object type's members); only a
        // top-level *literal source* widens, which never recurses into here.
        TypeTag::Object => {
            // Break a cycle: a recursive object already being rendered is `...`.
            if rendering.contains(&id) {
                return "...".to_string();
            }
            match store.object_type(id) {
                Some(obj) if obj.properties.is_empty() => "{}".to_string(),
                Some(obj) => {
                    rendering.push(id);
                    let members: Vec<String> = obj
                        .properties
                        .iter()
                        .map(|p| {
                            format!("{}: {}", p.name, render_type_inner(store, p.ty, false, rendering))
                        })
                        .collect();
                    rendering.pop();
                    format!("{{ {} }}", members.join("; "))
                }
                // Defensive fallback; an object always has a side-table entry.
                None => "<unsupported>".to_string(),
            }
        }
        // Function: `(x: number) => string` — parameters as `name: type`,
        // `, `-separated, always parenthesized, then ` => ` and the return type
        // (README "Type display format"). Parameter and return types never widen
        // (only a top-level literal *source* widens, which never recurses here).
        TypeTag::Function => match store.function_type(id) {
            Some(func) => {
                let params: Vec<String> = func
                    .params
                    .iter()
                    .map(|p| {
                        format!("{}: {}", p.name, render_type_inner(store, p.ty, false, rendering))
                    })
                    .collect();
                let ret = render_type_inner(store, func.ret, false, rendering);
                format!("({}) => {}", params.join(", "), ret)
            }
            // Defensive fallback; a function always has a side-table entry.
            None => "<unsupported>".to_string(),
        },
        // Union: `number | string` — members in stored (canonical, TypeId-sorted)
        // order, ` | `-separated (README "Type display format"). That order is
        // intern-order dependent, so union-typed targets are asserted code-only in
        // the corpus. Members never widen (only a top-level literal *source*
        // widens, which never recurses here).
        TypeTag::Union => match store.union_members(id) {
            Some(members) => {
                let parts: Vec<String> = members
                    .iter()
                    .map(|&m| render_type_inner(store, m, false, rendering))
                    .collect();
                parts.join(" | ")
            }
            // Defensive fallback; a union always has a side-table entry.
            None => "<unsupported>".to_string(),
        },
    }
}

fn render_literal(lit: &crate::types::repr::LiteralValue) -> String {
    use crate::types::repr::LiteralValue;
    match lit {
        LiteralValue::String(s) => format!("\"{s}\""),
        LiteralValue::Boolean(b) => b.to_string(),
        LiteralValue::Number(n) => {
            // Render integers without a trailing `.0` to match `tsc`'s literal
            // display (`1`, not `1.0`).
            if n.fract() == 0.0 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
    }
}

/// Render a batch of diagnostics for one file to a writer (the CLI uses stderr),
/// via `codespan-reporting`. Returns an IO/formatting error rather than
/// panicking. Color is intentionally off (plain writer) for stable, dependency-
/// light output.
pub fn render_to_writer(
    writer: &mut impl std::io::Write,
    file_name: &str,
    source: &str,
    diagnostics: &[Diagnostic],
) -> std::io::Result<()> {
    let file = SimpleFile::new(file_name, source);
    let config = Config::default();
    for diag in diagnostics {
        let cs = to_codespan(diag);
        // The only `Error` cases here are a missing file id (we always pass id
        // `()`) or an IO failure; map both to an IO error for the caller.
        term::emit_to_io_write(writer, &config, &file, &cs)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    Ok(())
}

/// Convert our structured diagnostic into a codespan-reporting one. `SimpleFile`
/// uses `()` as its file id.
fn to_codespan(diag: &Diagnostic) -> CsDiagnostic<()> {
    let base = match diag.severity {
        Severity::Error => CsDiagnostic::error(),
        Severity::Warning => CsDiagnostic::warning(),
    };
    base.with_code(diag.code.as_str())
        .with_message(diag.message.clone())
        .with_labels(vec![Label::primary((), diag.span.range())])
}
