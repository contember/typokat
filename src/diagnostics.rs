//! Diagnostics: the structured `Diagnostic` the checker emits and the conformance
//! harness consumes, plus terminal rendering via `codespan-reporting`.
//!
//! The harness consumes *structured* diagnostics (code + message + span), not
//! rendered text (mvp-plan §2/§3), so the data model is the source of truth and
//! the terminal rendering is a separate, presentation-only step.

use crate::relate::Reason;
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
    /// Property is private (accessed outside its declaring class) — M13.
    TK2341,
    /// Argument type is not assignable to the parameter type (call argument).
    TK2345,
    /// Object literal may only specify known properties (excess property).
    TK2353,
    /// Property is protected (accessed outside the class and its subclasses) —
    /// M13.
    TK2445,
    /// Cannot create an instance of an abstract class — M15.
    TK2511,
    /// Cannot assign to a read-only property — M14.
    TK2540,
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
            DiagnosticCode::TK2341 => "TK2341",
            DiagnosticCode::TK2345 => "TK2345",
            DiagnosticCode::TK2353 => "TK2353",
            DiagnosticCode::TK2445 => "TK2445",
            DiagnosticCode::TK2511 => "TK2511",
            DiagnosticCode::TK2540 => "TK2540",
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
    /// The nested reason-chain elaboration (M6, §6.4): the tsc-style "because…"
    /// lines rendered *below* the headline `message`, each already prefixed with
    /// its indentation, leaf last. Empty when there is no sub-reason — in
    /// particular a single-`Leaf` relation failure, whose headline already *is*
    /// the leaf, carries no elaboration so the rendered text stays exactly the
    /// one headline line (no earlier-milestone regression). Threaded into both
    /// [`Diagnostic::rendered_text`] (the harness substring-matches it) and the
    /// terminal renderer (codespan notes).
    elaboration: Vec<String>,
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
            elaboration: Vec::new(),
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
            elaboration: Vec::new(),
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
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2341` "property is private" error (M13): a `private` member
    /// accessed outside its declaring class. The primary span is the property name.
    pub fn property_is_private(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2341,
            severity: Severity::Error,
            message: format!("Property '{name}' is private and only accessible within class."),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2445` "property is protected" error (M13): a `protected`
    /// member accessed outside the declaring class and its subclasses. The primary
    /// span is the property name.
    pub fn property_is_protected(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2445,
            severity: Severity::Error,
            message: format!(
                "Property '{name}' is protected and only accessible within class and its subclasses."
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2511` "cannot create an instance of an abstract class" error
    /// (M15): a `new C(...)` whose directly-named class `C` is declared `abstract`.
    /// The primary span is the `new` expression. Only the named class's own
    /// abstractness matters — a concrete subclass of an abstract class instantiates
    /// fine.
    pub fn abstract_instantiation(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2511,
            severity: Severity::Error,
            message: "Cannot create an instance of an abstract class".to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2540` "cannot assign to a read-only property" error (M14): an
    /// assignment whose target is a `readonly` member, outside the one place it is
    /// allowed (the declaring class's constructor via `this.prop`). The primary span
    /// is the assignment target (the member expression).
    pub fn readonly_assignment(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2540,
            severity: Severity::Error,
            message: format!("Cannot assign to '{name}' because it is a read-only property"),
            span,
            elaboration: Vec::new(),
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
            elaboration: Vec::new(),
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
            elaboration: Vec::new(),
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
            elaboration: Vec::new(),
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
            elaboration: Vec::new(),
        }
    }

    /// Attach a rendered reason-chain elaboration (M6, §6.4) to this diagnostic.
    /// `lines` are the nested "because…" lines (already indented, leaf last) shown
    /// below the headline `message`. An empty `lines` leaves the diagnostic
    /// unchanged — single-`Leaf` failures pass an empty list so their rendered
    /// text stays exactly the one headline line.
    pub fn with_elaboration(mut self, lines: Vec<String>) -> Self {
        self.elaboration = lines;
        self
    }

    /// Whether this diagnostic counts as an error (drives the process exit code).
    pub fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }

    /// The fully-rendered diagnostic text the harness substring-matches against
    /// (`tests/cases/README.md`: case-sensitive `contains` over the rendered
    /// text, "including any nested reason chain"). The headline is `code +
    /// message`; M6 appends the nested reason-chain elaboration (each line on its
    /// own line, already indented) so the leaf cause is present in the matched
    /// text. With no elaboration this is exactly the one headline line.
    pub fn rendered_text(&self) -> String {
        let mut text = format!("error[{}]: {}", self.code.as_str(), self.message);
        for line in &self.elaboration {
            text.push('\n');
            text.push_str(line);
        }
        text
    }
}

/// Indentation step for one level of reason-chain nesting (two spaces, tsc-style).
const REASON_INDENT: &str = "  ";

/// Render a relation-failure reason chain (M6, §6.4) into the nested "because…"
/// elaboration lines shown *below* the diagnostic headline, leaf last.
///
/// The headline already states the outermost `(headline_src → tgt)` mismatch
/// (built by the checker). This renderer therefore emits only the *sub*-reason:
///
///  - a **terminal** head (`Leaf`, `MissingProperty`, `NoUnionMember`,
///    `ParameterCount`) is fully expressed by the headline already, so it yields
///    **no** lines — this is what keeps a single-`Leaf` failure (the M0–M5 scalar
///    mismatches) to exactly its one headline line, with no redundant wrapper;
///  - a **union-source** head descends straight into the offending member's own
///    reason, because the headline already names that member (`headline_src`);
///  - a **wrapper** head (`Property`, `Parameter`, `ReturnType`) renders its
///    "…are incompatible." line and then its nested cause, indented one level,
///    recursing down to the leaf.
pub fn render_reason_chain(store: &Store, head: &Reason) -> Vec<String> {
    match head {
        // The headline already states this mismatch in full.
        Reason::Leaf { .. }
        | Reason::MissingProperty { .. }
        | Reason::NoUnionMember { .. }
        | Reason::ParameterCount { .. }
        // M18: a tuple length mismatch is terminal — the headline states the two
        // tuple types and the cause is just "their lengths differ", so no extra line.
        | Reason::TupleLength { .. } => Vec::new(),
        // The headline names the offending union member (the checker's
        // `headline_src`), so the elaboration is *that member's* reason — exactly
        // as if the member's reason were itself the head.
        Reason::UnionSourceMember { because, .. } => render_reason_chain(store, because),
        // Structural wrappers: emit the "…are incompatible." line plus the nested
        // cause, starting one indent level in (the headline sits at level 0).
        Reason::Property { .. } | Reason::Parameter { .. } | Reason::ReturnType { .. } => {
            reason_lines(store, head, 1)
        }
        // An array-element mismatch (M17): the headline already states the
        // `S[]`/`T[]` mismatch, so the elaboration is the **element's** own cause,
        // one indent level in. A nested-array element prints its `S[]`/`T[]` line and
        // recurses; a union-element descends straight into the offending member (no
        // redundant wrapper) — both via [`element_reason_lines`].
        Reason::ArrayElement { because, .. } => element_reason_lines(store, because, 1),
        // M18: a tuple-element mismatch — the headline already states the `[…]`/`[…]`
        // mismatch, so the elaboration is the offending **element's** own cause, one
        // indent level in (a union element descends straight into its member, like the
        // array case). The position is implicit in the headline tuples.
        Reason::TupleElement { because, .. } => element_reason_lines(store, because, 1),
    }
}

/// Render the **element cause** of an array mismatch (M17) at indentation `depth`.
/// This is the array-element analogue of the head dispatch in [`render_reason_chain`]:
/// the enclosing array line (the headline, or an outer array's element line) already
/// states the `S[]`/`T[]` mismatch, so a **union-member** cause descends straight into
/// the offending member (avoiding the "member line + identical leaf line" doubling that
/// the shared [`reason_lines`] union arm produces when nested), while every other cause —
/// a nested array, a structural wrapper, or a terminal leaf — is rendered by
/// [`reason_lines`] exactly as anywhere else (so `string[][]` still nests its inner
/// `string[]`/`number[]` line, then the leaf).
fn element_reason_lines(store: &Store, cause: &Reason, depth: usize) -> Vec<String> {
    match cause {
        // A union element: the offending member is the cause; descend into it so a
        // scalar member renders one line (its own), not two identical ones.
        Reason::UnionSourceMember { because, .. } => element_reason_lines(store, because, depth),
        // Everything else renders normally (a nested `ArrayElement` prints its array
        // line and recurses; a leaf prints the leaf line).
        other => reason_lines(store, other, depth),
    }
}

/// Render `reason` and everything nested beneath it as indented lines, each at
/// `depth` levels of indentation (the leaf line last). Every `Reason` variant is
/// handled exhaustively; the recursion descends into the `because` of each
/// wrapper, so a chain renders as a top-down "Types of property…/Type … is not
/// assignable…" cascade. Never panics.
fn reason_lines(store: &Store, reason: &Reason, depth: usize) -> Vec<String> {
    let indent = REASON_INDENT.repeat(depth);
    match reason {
        // The base mismatch. Source widened (literal → base) to match the headline
        // rendering convention; target as-is.
        Reason::Leaf { src, tgt } => {
            let src = render_type(store, *src, /* widen */ true);
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )]
        }
        // A present-but-incompatible property: announce it, then nest its cause.
        Reason::Property { name, because, .. } => {
            let mut lines = vec![format!("{indent}Types of property '{name}' are incompatible.")];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // A required target property the source lacks (terminal).
        Reason::MissingProperty { name, tgt, .. } => {
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!("{indent}Property '{name}' is missing in type '{tgt}'.")]
        }
        // A contravariantly-incompatible parameter: name it (from the source
        // signature when available, else by position), then nest its cause.
        Reason::Parameter {
            index,
            src,
            tgt,
            because,
        } => {
            let src_name = parameter_name_at(store, *src, *index);
            let tgt_name = parameter_name_at(store, *tgt, *index);
            let header = match (src_name, tgt_name) {
                (Some(s), Some(t)) if s == t => {
                    format!("{indent}Types of parameters '{s}' are incompatible.")
                }
                (Some(s), Some(t)) => {
                    format!("{indent}Types of parameters '{s}' and '{t}' are incompatible.")
                }
                _ => format!(
                    "{indent}Types of parameters at position {} are incompatible.",
                    index + 1
                ),
            };
            let mut lines = vec![header];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // Covariantly-incompatible return types: announce, then nest the cause.
        Reason::ReturnType { because, .. } => {
            let mut lines = vec![format!("{indent}Call signature return types are incompatible.")];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // A union source whose member fails: announce the member, then nest its
        // cause. (At the chain head this arm is bypassed by `render_reason_chain`,
        // which descends straight into `because`; this arm renders a union nested
        // *inside* another reason.)
        Reason::UnionSourceMember {
            member,
            tgt,
            because,
            ..
        } => {
            let member = render_type(store, *member, /* widen */ true);
            let tgt = render_type(store, *tgt, /* widen */ false);
            let mut lines = vec![format!(
                "{indent}Type '{member}' is not assignable to type '{tgt}'."
            )];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // A source assignable to no member of a union target (terminal).
        Reason::NoUnionMember { src, tgt } => {
            let src = render_type(store, *src, /* widen */ true);
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )]
        }
        // Mismatched arity (terminal). M3 has no optional/rest params, so a
        // surplus source parameter is genuinely unsatisfiable.
        Reason::ParameterCount { src, tgt } => {
            let src = render_type(store, *src, /* widen */ false);
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!(
                "{indent}Type '{src}' provides more parameters than type '{tgt}' expects."
            )]
        }
        // An array whose element fails (M17): announce the two array types, then
        // nest the element's cause. (At the chain head this arm is bypassed by
        // `render_reason_chain`, which descends straight into `because`; this arm
        // renders an array mismatch nested *inside* another reason.)
        Reason::ArrayElement { src, tgt, because } => {
            let src = render_type(store, *src, /* widen */ false);
            let tgt = render_type(store, *tgt, /* widen */ false);
            let mut lines = vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // A tuple length mismatch (M18, terminal). The two tuple types render in full;
        // the cause is that their lengths differ (`[A, B]` vs `[A]`).
        Reason::TupleLength { src, tgt } => {
            let src = render_type(store, *src, /* widen */ false);
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )]
        }
        // A tuple whose element at a position fails (M18): announce the two tuple
        // types, then nest the element's cause. (At the chain head this arm is
        // bypassed by `render_reason_chain`, which descends straight into `because`;
        // this arm renders a tuple mismatch nested *inside* another reason.)
        Reason::TupleElement {
            src, tgt, because, ..
        } => {
            let src = render_type(store, *src, /* widen */ false);
            let tgt = render_type(store, *tgt, /* widen */ false);
            let mut lines = vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
    }
}

/// The name of the parameter at `index` of a function-typed id, if `id` is a
/// function type with that position. Used to phrase a `Parameter` reason; falls
/// back to `None` (positional phrasing) for anything else.
fn parameter_name_at(store: &Store, id: TypeId, index: usize) -> Option<String> {
    let func = store.function_type(id)?;
    func.params.get(index).map(|p| p.name.clone())
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
        // Type parameter (M9): render its source name (`T`). A type parameter only
        // surfaces in a message for an *uninstantiated* generic (out of the M9
        // fixtures' explicit-args path, where every parameter is substituted away);
        // the name is display-only and not part of the type's identity.
        TypeTag::TypeParam => store
            .type_param(id)
            .map(|p| p.name.clone())
            // Defensive fallback; a type parameter always has a side-table entry.
            .unwrap_or_else(|| "unknown".to_string()),
        // Array (M17): `<elem>[]`. The element is parenthesized where the bare
        // postfix `[]` would otherwise bind ambiguously — a **union** or **function**
        // element (`(number | string)[]`, `((x: number) => string)[]`) — matching
        // tsc's display. Intrinsics, literals, objects, type parameters, and nested
        // arrays need no parentheses. The element never widens (only a top-level
        // literal *source* widens, which never recurses here).
        TypeTag::Array => match store.array_type(id) {
            Some(array) => {
                let elem = render_type_inner(store, array.element, false, rendering);
                if array_element_needs_parens(store, array.element) {
                    format!("({elem})[]")
                } else {
                    format!("{elem}[]")
                }
            }
            // Defensive fallback; an array always has a side-table entry.
            None => "<unsupported>".to_string(),
        },
        // Tuple (M18): `[A, B]` — elements in source order, `, `-separated, wrapped in
        // square brackets (README "Type display format"). The empty tuple renders as
        // `[]`. Elements never widen (only a top-level literal *source* widens, which
        // never recurses here); no element ever needs parenthesizing (the `[…]`
        // brackets already delimit each).
        TypeTag::Tuple => match store.tuple_type(id) {
            Some(tuple) => {
                let elems: Vec<String> = tuple
                    .elements
                    .iter()
                    .map(|&e| render_type_inner(store, e, false, rendering))
                    .collect();
                format!("[{}]", elems.join(", "))
            }
            // Defensive fallback; a tuple always has a side-table entry.
            None => "<unsupported>".to_string(),
        },
    }
}

/// Whether an array element type must be **parenthesized** before the postfix `[]`
/// (M17). A union (`number | string`) or function (`(x: number) => string`) element
/// would bind ambiguously under bare `[]`, so it is wrapped (`(number | string)[]`).
/// Everything else — intrinsics, literals, objects, type parameters, nested arrays —
/// renders without parentheses.
fn array_element_needs_parens(store: &Store, element: TypeId) -> bool {
    matches!(store.tag(element), TypeTag::Union | TypeTag::Function)
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
/// uses `()` as its file id. The reason-chain elaboration (M6) is attached as a
/// trailing note so the nested "because…" cascade shows under the primary label
/// in the human-facing CLI output, readably indented (already done in the lines).
fn to_codespan(diag: &Diagnostic) -> CsDiagnostic<()> {
    let base = match diag.severity {
        Severity::Error => CsDiagnostic::error(),
        Severity::Warning => CsDiagnostic::warning(),
    };
    let cs = base
        .with_code(diag.code.as_str())
        .with_message(diag.message.clone())
        .with_labels(vec![Label::primary((), diag.span.range())]);
    if diag.elaboration.is_empty() {
        cs
    } else {
        cs.with_notes(vec![diag.elaboration.join("\n")])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{
        FunctionType, LiteralValue, ObjectType, ParameterType, PropertyType,
    };
    use crate::types::Interner;

    fn prop(name: &str, ty: TypeId) -> PropertyType {
        PropertyType::public(name, ty)
    }

    /// A single `Leaf` head — the M0–M5 scalar mismatch — renders **no**
    /// elaboration: the headline already states it, so there is no redundant
    /// "Types of …" wrapper and the rendered diagnostic stays exactly one line.
    /// This is the no-regression guarantee for the earlier milestones.
    #[test]
    fn single_leaf_chain_has_no_elaboration() {
        let mut interner = Interner::with_intrinsics();
        let lit_str = interner.intern_literal(LiteralValue::String("x".to_string()));
        let wk = interner.well_known();
        let store = interner.store();

        let head = Reason::Leaf {
            src: lit_str,
            tgt: wk.number,
        };
        let lines = render_reason_chain(store, &head);
        assert!(
            lines.is_empty(),
            "a single-Leaf chain must produce no elaboration lines, got {lines:?}"
        );

        // And through a diagnostic: the rendered text is the one headline line,
        // unchanged from M0 (the leaf substring lives in the headline itself).
        let diag = Diagnostic::not_assignable(
            Span::new(0, 1),
            "Type 'string' is not assignable to type 'number'".to_string(),
        )
        .with_elaboration(lines);
        assert_eq!(
            diag.rendered_text(),
            "error[TK2322]: Type 'string' is not assignable to type 'number'"
        );
        assert!(!diag.rendered_text().contains('\n'));
    }

    /// A nested `Property → Property → Leaf` chain renders the tsc-style cascade,
    /// indented one level per depth, with the scalar leaf line **present** and
    /// last. This is the M6 nested-object (`TK2322`) shape from `nested.ts`.
    #[test]
    fn nested_property_chain_renders_leaf_last() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // { a: { b: string } } (src) vs { a: { b: number } } (tgt).
        let inner_str = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.string)],
        });
        let inner_num = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.number)],
        });
        let outer_src = interner.intern_object(ObjectType {
            properties: vec![prop("a", inner_str)],
        });
        let outer_tgt = interner.intern_object(ObjectType {
            properties: vec![prop("a", inner_num)],
        });
        let store = interner.store();

        // The reason chain the relation engine builds for this failure.
        let head = Reason::Property {
            name: "a".to_string(),
            src: outer_src,
            tgt: outer_tgt,
            because: Box::new(Reason::Property {
                name: "b".to_string(),
                src: inner_str,
                tgt: inner_num,
                because: Box::new(Reason::Leaf {
                    src: wk.string,
                    tgt: wk.number,
                }),
            }),
        };

        let lines = render_reason_chain(store, &head);
        assert_eq!(
            lines,
            vec![
                "  Types of property 'a' are incompatible.".to_string(),
                "    Types of property 'b' are incompatible.".to_string(),
                "      Type 'string' is not assignable to type 'number'.".to_string(),
            ]
        );

        // The leaf substring asserted by the corpus must be in the fully rendered
        // diagnostic text (headline + elaboration).
        let rendered = Diagnostic::not_assignable(
            Span::new(0, 1),
            "Type '{ a: { b: string } }' is not assignable to type '{ a: { b: number } }'"
                .to_string(),
        )
        .with_elaboration(lines)
        .rendered_text();
        assert!(rendered.contains("Type 'string' is not assignable to type 'number'"));
    }

    /// A `MissingProperty` head is terminal — the headline (`TK2741`) states the
    /// missing property in full — so it renders no elaboration.
    #[test]
    fn missing_property_head_has_no_elaboration() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let a_only = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number)],
        });
        let ab = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number), prop("b", wk.string)],
        });
        let store = interner.store();

        let head = Reason::MissingProperty {
            name: "b".to_string(),
            src: a_only,
            tgt: ab,
        };
        assert!(render_reason_chain(store, &head).is_empty());
    }

    /// A function `Parameter → Leaf` chain names the parameter (from the source
    /// signature) and nests its leaf cause underneath.
    #[test]
    fn parameter_chain_names_parameter_and_nests_leaf() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        let str_to_num = interner.intern_function(FunctionType {
            params: vec![ParameterType {
                name: "x".to_string(),
                ty: wk.string,
                optional: false,
            }],
            ret: wk.number,
        });
        let num_to_num = interner.intern_function(FunctionType {
            params: vec![ParameterType {
                name: "x".to_string(),
                ty: wk.number,
                optional: false,
            }],
            ret: wk.number,
        });
        let store = interner.store();

        // Contravariant parameter failure: tgt param `number` not assignable to
        // src param `string` (the engine builds the leaf in the tgt→src order).
        let head = Reason::Parameter {
            index: 0,
            src: str_to_num,
            tgt: num_to_num,
            because: Box::new(Reason::Leaf {
                src: wk.number,
                tgt: wk.string,
            }),
        };
        let lines = render_reason_chain(store, &head);
        assert_eq!(
            lines,
            vec![
                "  Types of parameters 'x' are incompatible.".to_string(),
                "    Type 'number' is not assignable to type 'string'.".to_string(),
            ]
        );
    }

    /// A union-source head descends straight into the offending member's reason
    /// (the headline already names that member via the checker's `headline_src`).
    /// When the member's cause is a scalar leaf, the elaboration is empty.
    #[test]
    fn union_source_head_descends_into_member_cause() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let union = interner.union(vec![wk.string, wk.number]);
        let store = interner.store();

        // `string | number` not assignable to `number`: the `string` member fails
        // with a scalar leaf. Headline reports `string`; elaboration is empty.
        let head = Reason::UnionSourceMember {
            member: wk.string,
            src: union,
            tgt: wk.number,
            because: Box::new(Reason::Leaf {
                src: wk.string,
                tgt: wk.number,
            }),
        };
        assert!(render_reason_chain(store, &head).is_empty());

        // But a union member with a *nested* cause renders that cause.
        let inner_str = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.string)],
        });
        let inner_num = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.number)],
        });
        let store = interner.store();
        let head = Reason::UnionSourceMember {
            member: inner_str,
            src: union,
            tgt: inner_num,
            because: Box::new(Reason::Property {
                name: "b".to_string(),
                src: inner_str,
                tgt: inner_num,
                because: Box::new(Reason::Leaf {
                    src: wk.string,
                    tgt: wk.number,
                }),
            }),
        };
        let lines = render_reason_chain(store, &head);
        assert_eq!(
            lines,
            vec![
                "  Types of property 'b' are incompatible.".to_string(),
                "    Type 'string' is not assignable to type 'number'.".to_string(),
            ]
        );
    }

    /// M17 array rendering: `number[]`, the nested `number[][]`, and the
    /// parenthesized `(number | string)[]` / `((x: number) => string)[]` element
    /// forms (a union/function element binds ambiguously under bare `[]`).
    #[test]
    fn array_type_renders_with_parenthesized_element() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        let num_arr = interner.intern_array(wk.number);
        assert_eq!(render_type(interner.store(), num_arr, false), "number[]");

        // Nested: no parentheses for an array element.
        let num_arr_arr = interner.intern_array(num_arr);
        assert_eq!(render_type(interner.store(), num_arr_arr, false), "number[][]");

        // A union element IS parenthesized.
        let union = interner.union(vec![wk.number, wk.string]);
        let union_arr = interner.intern_array(union);
        let rendered = render_type(interner.store(), union_arr, false);
        assert!(
            rendered == "(number | string)[]" || rendered == "(string | number)[]",
            "a union element must be parenthesized, got {rendered:?}"
        );

        // A function element IS parenthesized.
        let func = interner.intern_function(FunctionType {
            params: vec![ParameterType {
                name: "x".to_string(),
                ty: wk.number,
                optional: false,
            }],
            ret: wk.string,
        });
        let func_arr = interner.intern_array(func);
        assert_eq!(
            render_type(interner.store(), func_arr, false),
            "((x: number) => string)[]"
        );
    }

    /// M18 tuple rendering: `[number, string]` (order preserved, `, `-separated,
    /// square-bracketed) and the empty tuple `[]`.
    #[test]
    fn tuple_type_renders_in_brackets() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        let num_str = interner.intern_tuple(vec![wk.number, wk.string]);
        assert_eq!(render_type(interner.store(), num_str, false), "[number, string]");

        // Order is preserved (not sorted): [string, number] renders in that order.
        let str_num = interner.intern_tuple(vec![wk.string, wk.number]);
        assert_eq!(render_type(interner.store(), str_num, false), "[string, number]");

        // The empty tuple renders as `[]`.
        let empty = interner.intern_tuple(vec![]);
        assert_eq!(render_type(interner.store(), empty, false), "[]");

        // A nested tuple element renders inline (the outer brackets delimit it).
        let nested = interner.intern_tuple(vec![num_str, wk.boolean]);
        assert_eq!(
            render_type(interner.store(), nested, false),
            "[[number, string], boolean]"
        );
    }

    /// M18 — a `TupleElement` head renders the offending element's nested cause (the
    /// headline already states the `[…]`/`[…]` mismatch). A scalar leaf element yields
    /// exactly one nested line; a `TupleLength` head (terminal) renders **none**.
    #[test]
    fn tuple_reason_heads_render_element_cause_and_length_terminal() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let str_num = interner.intern_tuple(vec![wk.string, wk.number]);
        let num_str = interner.intern_tuple(vec![wk.number, wk.string]);
        let num_only = interner.intern_tuple(vec![wk.number]);
        let store = interner.store();

        // [string, number] not assignable to [number, string]: position 0 fails
        // (string→number) — one nested line.
        let head = Reason::TupleElement {
            index: 0,
            src: str_num,
            tgt: num_str,
            because: Box::new(Reason::Leaf {
                src: wk.string,
                tgt: wk.number,
            }),
        };
        assert_eq!(
            render_reason_chain(store, &head),
            vec!["  Type 'string' is not assignable to type 'number'.".to_string()]
        );

        // A length mismatch is terminal — the headline states the two tuple types in
        // full, so no elaboration.
        let len_head = Reason::TupleLength {
            src: num_only,
            tgt: num_str,
        };
        assert!(render_reason_chain(store, &len_head).is_empty());
    }

    /// M17 — an `ArrayElement` head renders the element's nested cause (the headline
    /// already states the `S[]`/`T[]` mismatch). A scalar leaf element yields exactly
    /// one nested line.
    #[test]
    fn array_element_head_renders_element_cause() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let str_arr = interner.intern_array(wk.string);
        let num_arr = interner.intern_array(wk.number);
        let store = interner.store();

        // string[] not assignable to number[]: the element `string`→`number` fails.
        let head = Reason::ArrayElement {
            src: str_arr,
            tgt: num_arr,
            because: Box::new(Reason::Leaf {
                src: wk.string,
                tgt: wk.number,
            }),
        };
        let lines = render_reason_chain(store, &head);
        assert_eq!(
            lines,
            vec!["  Type 'string' is not assignable to type 'number'.".to_string()]
        );
    }

    /// M17 — a **union-element** array mismatch (`(number | string)[]` not assignable
    /// to `number[]`) renders a **single** nested line (the offending member), not the
    /// doubled "member line + identical leaf" the shared `reason_lines` union arm would
    /// produce when nested. This pins the `element_reason_lines` collapse.
    #[test]
    fn array_union_element_renders_single_line() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let union = interner.union(vec![wk.number, wk.string]);
        let union_arr = interner.intern_array(union);
        let num_arr = interner.intern_array(wk.number);
        let store = interner.store();

        // `(number | string)[]` not assignable to `number[]`: the element's `string`
        // member fails (UnionSourceMember → Leaf).
        let head = Reason::ArrayElement {
            src: union_arr,
            tgt: num_arr,
            because: Box::new(Reason::UnionSourceMember {
                member: wk.string,
                src: union,
                tgt: wk.number,
                because: Box::new(Reason::Leaf {
                    src: wk.string,
                    tgt: wk.number,
                }),
            }),
        };
        let lines = render_reason_chain(store, &head);
        assert_eq!(
            lines,
            vec!["  Type 'string' is not assignable to type 'number'.".to_string()],
            "a union-element array mismatch must render a single nested line, not a doubled one"
        );
    }

    /// M17 — a **nested-array** mismatch (`string[][]` not assignable to `number[][]`)
    /// nests each array level: the inner `string[]`/`number[]` line, then the scalar
    /// leaf, each one indent deeper. Pins that `ArrayElement` is NOT a pure
    /// head-collapse (it prints intermediate array lines).
    #[test]
    fn array_nested_element_nests_each_level() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let str_arr = interner.intern_array(wk.string);
        let num_arr = interner.intern_array(wk.number);
        let str_arr_arr = interner.intern_array(str_arr);
        let num_arr_arr = interner.intern_array(num_arr);
        let store = interner.store();

        let head = Reason::ArrayElement {
            src: str_arr_arr,
            tgt: num_arr_arr,
            because: Box::new(Reason::ArrayElement {
                src: str_arr,
                tgt: num_arr,
                because: Box::new(Reason::Leaf {
                    src: wk.string,
                    tgt: wk.number,
                }),
            }),
        };
        let lines = render_reason_chain(store, &head);
        assert_eq!(
            lines,
            vec![
                "  Type 'string[]' is not assignable to type 'number[]'.".to_string(),
                "    Type 'string' is not assignable to type 'number'.".to_string(),
            ]
        );
    }
}
