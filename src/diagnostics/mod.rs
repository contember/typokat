//! Structured checker diagnostics plus terminal rendering.
//!
//! The conformance harness consumes code/message/span as the source of truth;
//! `codespan-reporting` output is presentation-only.

mod incomplete;
mod reason;
mod render_type;
mod writer;

pub use incomplete::{
    render_incomplete_to_writer, render_incomplete_to_writer_with_format, IncompleteSurface,
};
pub use reason::render_reason_chain;
pub use render_type::render_type;
pub use writer::{render_to_writer, render_to_writer_with_format};

use crate::span::Span;

/// A tsc-compatible diagnostic code (numbers reused from tsc, `TK` prefix to be
/// honest about source — mvp-plan §2). Only the codes the MVP can emit are
/// listed; later milestones add variants.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DiagnosticCode {
    /// Static member references its containing class's type parameter.
    TK2302,
    /// Cannot find name (unresolved identifier).
    TK2304,
    /// Module has no exported member.
    TK2305,
    /// Cannot find module.
    TK2307,
    /// Type parameter has a circular constraint (`<T extends T>`) — M24.
    TK2313,
    /// Type X is not assignable to type Y.
    TK2322,
    /// Property does not exist on type (member access).
    TK2339,
    /// Property is private (accessed outside its declaring class) — M13.
    TK2341,
    /// Type argument does not satisfy the type parameter's constraint — M24.
    TK2344,
    /// Type alias circularly references itself — M25 (`type A = A extends … ? … : …`).
    TK2456,
    /// Argument type is not assignable to the parameter type (call argument).
    TK2345,
    /// Object literal may only specify known properties (excess property).
    TK2353,
    /// Function implementation is missing or not immediately following the declaration.
    TK2391,
    /// Overload signature is not compatible with its implementation signature.
    TK2394,
    /// Property in a derived type is not assignable to the same property in the
    /// base type (override compatibility) — backlog 06.
    TK2416,
    /// Property is protected (accessed outside the class and its subclasses) —
    /// M13.
    TK2445,
    /// Cannot create an instance of an abstract class — M15.
    TK2511,
    /// Non-abstract class does not implement one inherited abstract member —
    /// backlog 06.
    TK2515,
    /// Non-abstract class is missing implementations for two or more inherited
    /// abstract members (aggregated) — backlog 06.
    TK2654,
    /// Constructor of class is private (constructed outside its declaring class) —
    /// backlog 20.
    TK2673,
    /// Constructor of class is protected (constructed outside the class and its
    /// subclasses) — backlog 20.
    TK2674,
    /// Cannot assign to a read-only property — M14.
    TK2540,
    /// Wrong number of call arguments (arity).
    TK2554,
    /// Too few call arguments for a variadic signature.
    TK2555,
    /// Wrong number of explicit type arguments.
    TK2558,
    /// Type instantiation is excessively deep and possibly infinite — M25.
    TK2589,
    /// Property is missing in type but required.
    TK2741,
    /// Type parameter defaults can only reference preceding type parameters.
    TK2744,
    /// No overload matches this call.
    TK2769,
}

impl DiagnosticCode {
    /// The rendered code string, e.g. `"TK2322"`.
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::TK2302 => "TK2302",
            DiagnosticCode::TK2304 => "TK2304",
            DiagnosticCode::TK2305 => "TK2305",
            DiagnosticCode::TK2307 => "TK2307",
            DiagnosticCode::TK2313 => "TK2313",
            DiagnosticCode::TK2322 => "TK2322",
            DiagnosticCode::TK2339 => "TK2339",
            DiagnosticCode::TK2341 => "TK2341",
            DiagnosticCode::TK2344 => "TK2344",
            DiagnosticCode::TK2345 => "TK2345",
            DiagnosticCode::TK2456 => "TK2456",
            DiagnosticCode::TK2353 => "TK2353",
            DiagnosticCode::TK2391 => "TK2391",
            DiagnosticCode::TK2394 => "TK2394",
            DiagnosticCode::TK2416 => "TK2416",
            DiagnosticCode::TK2445 => "TK2445",
            DiagnosticCode::TK2511 => "TK2511",
            DiagnosticCode::TK2515 => "TK2515",
            DiagnosticCode::TK2654 => "TK2654",
            DiagnosticCode::TK2673 => "TK2673",
            DiagnosticCode::TK2674 => "TK2674",
            DiagnosticCode::TK2540 => "TK2540",
            DiagnosticCode::TK2554 => "TK2554",
            DiagnosticCode::TK2555 => "TK2555",
            DiagnosticCode::TK2558 => "TK2558",
            DiagnosticCode::TK2589 => "TK2589",
            DiagnosticCode::TK2741 => "TK2741",
            DiagnosticCode::TK2744 => "TK2744",
            DiagnosticCode::TK2769 => "TK2769",
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

/// Human-facing terminal rendering mode. `Rich` is the default CLI output;
/// `Compact` is a lower-overhead, tsc-style line format useful for benchmarks.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DiagnosticFormat {
    Rich,
    Compact,
}

/// A structured diagnostic. `span` is the **primary** span — its start is what
/// the harness maps to a 1-based line for marker matching.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    /// Nested reason-chain lines rendered below the headline, already indented
    /// and leaf last. A single-`Leaf` failure stays headline-only; both the
    /// harness text and terminal notes consume the same elaboration.
    elaboration: Vec<String>,
}

impl Diagnostic {
    /// Construct a `TK2302` static-member class-binder error.
    pub fn static_member_references_class_type_parameter(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2302,
            severity: Severity::Error,
            message: "Static members cannot reference class type parameters.".to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

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

    /// Construct a `TK2305` "module has no exported member" error.
    pub fn no_exported_member(span: Span, module: &str, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2305,
            severity: Severity::Error,
            message: format!("Module '{module}' has no exported member '{name}'"),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2307` "cannot find module" error.
    pub fn cannot_find_module(span: Span, module: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2307,
            severity: Severity::Error,
            message: format!("Cannot find module '{module}'"),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2313` circular-constraint error for a bare type-parameter
    /// constraint cycle. The parameter records no constraint, so the degenerate
    /// cycle never reaches the relation engine's assume-true stack.
    pub fn circular_constraint(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2313,
            severity: Severity::Error,
            message: format!("Type parameter '{name}' has a circular constraint."),
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

    /// Construct a `TK2511` error for `new C(...)` when the directly named class
    /// is abstract; a concrete subclass of an abstract class instantiates fine.
    pub fn abstract_instantiation(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2511,
            severity: Severity::Error,
            message: "Cannot create an instance of an abstract class".to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2416` override-compatibility error, with the nested reason
    /// chain attached exactly as `TK2322`.
    pub fn property_override_incompatible(
        span: Span,
        name: &str,
        derived: &str,
        base: &str,
    ) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2416,
            severity: Severity::Error,
            message: format!(
                "Property '{name}' in type '{derived}' is not assignable to the same property in base type '{base}'."
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2515` error for one unimplemented inherited abstract member.
    /// `base` is the direct base name, and the member renders unquoted.
    pub fn missing_abstract_member(span: Span, class_name: &str, member: &str, base: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2515,
            severity: Severity::Error,
            message: format!(
                "Non-abstract class '{class_name}' does not implement inherited abstract member {member} from class '{base}'."
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2654` error for multiple unimplemented inherited abstract
    /// members. Members render quoted and comma-separated in pending-list order.
    pub fn missing_abstract_members(
        span: Span,
        class_name: &str,
        members: &[String],
        base: &str,
    ) -> Self {
        let list = members
            .iter()
            .map(|m| format!("'{m}'"))
            .collect::<Vec<_>>()
            .join(", ");
        Diagnostic {
            code: DiagnosticCode::TK2654,
            severity: Severity::Error,
            message: format!(
                "Non-abstract class '{class_name}' is missing implementations for the following members of '{base}': {list}."
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2673` "constructor is private" error (backlog 20): a direct
    /// `new C(...)` reaches a `private` constructor from outside its declaring class.
    /// `class_name` is the constructor's **declaring** class (the base, for an
    /// inherited constructor). The primary span is the whole `new` expression.
    pub fn constructor_is_private(span: Span, class_name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2673,
            severity: Severity::Error,
            message: format!(
                "Constructor of class '{class_name}' is private and only accessible within the class declaration."
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2674` protected-constructor error for an invalid direct
    /// `new C(...)`; `class_name` is the constructor's declaring class.
    pub fn constructor_is_protected(span: Span, class_name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2674,
            severity: Severity::Error,
            message: format!(
                "Constructor of class '{class_name}' is protected and only accessible within the class declaration."
            ),
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

    /// Construct a `TK2456` "type alias circularly references itself" error (M25): a
    /// type alias whose conditional-type body's **check** surface-references the alias
    /// itself (`type Self = Self extends string ? 1 : 2`). The primary span is the alias
    /// declaration name. The alias degrades to the error type (silent downstream).
    pub fn circular_type_alias(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2456,
            severity: Severity::Error,
            message: format!("Type alias '{name}' circularly references itself."),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2589` "excessively deep" error (M25): evaluating a conditional
    /// type exceeded the per-root instantiation step budget (a runaway / genuinely
    /// infinite type). The primary span is the annotation that demanded the evaluation
    /// (a documented divergence from tsc, which attributes it inside the alias body).
    pub fn excessively_deep(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2589,
            severity: Severity::Error,
            message: "Type instantiation is excessively deep and possibly infinite.".to_string(),
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

    /// Construct a `TK2353` excess-property error for a fresh object literal
    /// property missing from the object target. The TS2353-compatible suffix is
    /// pinned by the corpus.
    pub fn excess_property(span: Span, name: &str, tgt: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2353,
            severity: Severity::Error,
            message: format!(
                "Object literal may only specify known properties, and '{name}' does not exist in type '{tgt}'."
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2394` overload implementation compatibility error.
    pub fn overload_incompatible(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2394,
            severity: Severity::Error,
            message: "This overload signature is not compatible with its implementation signature."
                .to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2391` invalid overload layout error.
    pub fn overload_missing_implementation(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2391,
            severity: Severity::Error,
            message:
                "Function implementation is missing or not immediately following the declaration."
                    .to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2344` "type does not satisfy the constraint" error (M24): an
    /// explicit type argument `src` violates its type parameter's constraint `tgt`.
    /// The primary span is the offending type argument; the nested reason chain is
    /// attached via [`Diagnostic::with_elaboration`], exactly like `TK2322`.
    pub fn constraint_not_satisfied(span: Span, src: &str, tgt: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2344,
            severity: Severity::Error,
            message: format!("Type '{src}' does not satisfy the constraint '{tgt}'."),
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

    /// Construct a `TK2554` arity error for optional/default parameter ranges.
    pub fn wrong_argument_count_range(span: Span, min: usize, max: usize, got: usize) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2554,
            severity: Severity::Error,
            message: format!("Expected {min}-{max} arguments, but got {got}"),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2555` rest-arity error for calls below the required minimum.
    pub fn wrong_min_argument_count(span: Span, min: usize, got: usize) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2555,
            severity: Severity::Error,
            message: format!("Expected at least {min} arguments, but got {got}"),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2558` arity error for explicit type arguments.
    pub fn wrong_type_argument_count(span: Span, min: usize, max: usize, got: usize) -> Self {
        let expected = if min == max {
            min.to_string()
        } else {
            format!("{min}-{max}")
        };
        Diagnostic {
            code: DiagnosticCode::TK2558,
            severity: Severity::Error,
            message: format!("Expected {expected} type arguments, but got {got}"),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2744` default-visibility error for a type parameter that
    /// references a later binder in the same declaration.
    pub fn type_parameter_default_forward_reference(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2744,
            severity: Severity::Error,
            message:
                "Type parameter defaults can only reference previously declared type parameters."
                    .to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2769` overload resolution failure.
    pub fn no_overload_matches(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2769,
            severity: Severity::Error,
            message: "No overload matches this call".to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Attach rendered reason-chain lines below the headline. Empty input leaves
    /// single-`Leaf` failures headline-only.
    pub fn with_elaboration(mut self, lines: Vec<String>) -> Self {
        self.elaboration = lines;
        self
    }

    /// Whether this diagnostic counts as an error (drives the process exit code).
    pub fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }

    /// Fully rendered diagnostic text for the harness substring match, including
    /// any nested reason-chain elaboration. Without elaboration this is exactly
    /// the headline line.
    pub fn rendered_text(&self) -> String {
        let mut text = format!("error[{}]: {}", self.code.as_str(), self.message);
        for line in &self.elaboration {
            text.push('\n');
            text.push_str(line);
        }
        text
    }
}

#[cfg(test)]
mod tests;
