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

use crate::binder::namespace::{QualifiedTypePathDeferredReason, QualifiedTypePathResolution};
use crate::span::Span;

/// A tsc-compatible diagnostic code (numbers reused from tsc, `TK` prefix to be
/// honest about source — mvp-plan §2). Only the codes the MVP can emit are
/// listed; later milestones add variants.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DiagnosticCode {
    /// `export as namespace` appears outside an external module.
    TK1314,
    /// `export as namespace` appears in a non-declaration source file.
    TK1315,
    TK1545,
    TK2300,
    /// Static member references its containing class's type parameter.
    TK2302,
    /// Cannot find name (unresolved identifier).
    TK2304,
    /// Module has no exported member.
    TK2305,
    /// Cannot find module.
    TK2307,
    /// An interface recursively references itself through its base types.
    TK2310,
    /// Type parameter has a circular constraint (`<T extends T>`) — M24.
    TK2313,
    /// Generic type requires an exact number of type arguments.
    TK2314,
    /// A non-generic type was supplied type arguments.
    TK2315,
    TK2320,
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
    /// A value with a known non-callable type is invoked.
    TK2349,
    /// A direct standalone namespace root is used as a constructor.
    TK2351,
    /// Object literal may only specify known properties (excess property).
    TK2353,
    /// Function implementation is missing or not immediately following the declaration.
    TK2391,
    /// Overload signatures disagree on public/private/protected accessibility.
    TK2385,
    /// Overload signature is not compatible with its implementation signature.
    TK2394,
    TK2374,
    TK2411,
    TK2413,
    /// Property in a derived type is not assignable to the same property in the
    /// base type (override compatibility) — backlog 06.
    TK2416,
    TK2428,
    /// An interface's complete surface is incompatible with one extended base.
    TK2430,
    /// A runtime namespace precedes the class or function it augments.
    TK2434,
    /// Property is protected (accessed outside the class and its subclasses) —
    /// M13.
    TK2445,
    /// Cannot redeclare a block-scoped variable.
    TK2451,
    /// Cannot find namespace.
    TK2503,
    /// Cannot create an instance of an abstract class — M15.
    TK2511,
    /// Non-abstract class does not implement one inherited abstract member —
    /// backlog 06.
    TK2515,
    /// Non-abstract class is missing implementations for two or more inherited
    /// abstract members (aggregated) — backlog 06.
    TK2654,
    /// A namespace binding is not an assignable variable.
    TK2631,
    /// An ambient export list references an alias output instead of a local declaration.
    TK2661,
    /// A global augmentation appears outside an external or ambient module.
    TK2669,
    /// A global augmentation is missing `declare` outside an ambient context.
    TK2670,
    /// Constructor of class is private (constructed outside its declaring class) —
    /// backlog 20.
    TK2673,
    /// Constructor of class is protected (constructed outside the class and its
    /// subclasses) — backlog 20.
    TK2674,
    /// The call receiver is not assignable to an explicit `this` parameter.
    TK2684,
    TK2687,
    /// Namespace has no exported member.
    TK2694,
    /// A type-only name is used as a namespace.
    TK2702,
    /// A required type parameter follows an optional type parameter.
    TK2706,
    /// Generic type requires a bounded range of type arguments.
    TK2707,
    /// A type-only path segment is accessed as a namespace.
    TK2713,
    /// A pure type-only namespace is used in value position.
    TK2708,
    TK2717,
    /// Cannot assign to a read-only property — M14.
    TK2540,
    /// Wrong number of call arguments (arity).
    TK2554,
    /// Too few call arguments for a variadic signature.
    TK2555,
    /// Wrong number of explicit type arguments.
    TK2558,
    /// A static class member is accessed through an instance.
    TK2576,
    /// Type instantiation is excessively deep and possibly infinite — M25.
    TK2589,
    /// Property is missing in type but required.
    TK2741,
    /// Type parameter defaults can only reference preceding type parameters.
    TK2744,
    /// A value is used as a type.
    TK2749,
    /// No overload matches this call.
    TK2769,
    TK2852,
}

impl DiagnosticCode {
    /// The rendered code string, e.g. `"TK2322"`.
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::TK1314 => "TK1314",
            DiagnosticCode::TK1315 => "TK1315",
            DiagnosticCode::TK1545 => "TK1545",
            DiagnosticCode::TK2300 => "TK2300",
            DiagnosticCode::TK2302 => "TK2302",
            DiagnosticCode::TK2304 => "TK2304",
            DiagnosticCode::TK2305 => "TK2305",
            DiagnosticCode::TK2307 => "TK2307",
            DiagnosticCode::TK2310 => "TK2310",
            DiagnosticCode::TK2313 => "TK2313",
            DiagnosticCode::TK2314 => "TK2314",
            DiagnosticCode::TK2315 => "TK2315",
            DiagnosticCode::TK2320 => "TK2320",
            DiagnosticCode::TK2322 => "TK2322",
            DiagnosticCode::TK2339 => "TK2339",
            DiagnosticCode::TK2341 => "TK2341",
            DiagnosticCode::TK2344 => "TK2344",
            DiagnosticCode::TK2345 => "TK2345",
            DiagnosticCode::TK2349 => "TK2349",
            DiagnosticCode::TK2351 => "TK2351",
            DiagnosticCode::TK2456 => "TK2456",
            DiagnosticCode::TK2353 => "TK2353",
            DiagnosticCode::TK2385 => "TK2385",
            DiagnosticCode::TK2391 => "TK2391",
            DiagnosticCode::TK2394 => "TK2394",
            DiagnosticCode::TK2374 => "TK2374",
            DiagnosticCode::TK2411 => "TK2411",
            DiagnosticCode::TK2413 => "TK2413",
            DiagnosticCode::TK2416 => "TK2416",
            DiagnosticCode::TK2428 => "TK2428",
            DiagnosticCode::TK2430 => "TK2430",
            DiagnosticCode::TK2434 => "TK2434",
            DiagnosticCode::TK2445 => "TK2445",
            DiagnosticCode::TK2451 => "TK2451",
            DiagnosticCode::TK2503 => "TK2503",
            DiagnosticCode::TK2511 => "TK2511",
            DiagnosticCode::TK2515 => "TK2515",
            DiagnosticCode::TK2654 => "TK2654",
            DiagnosticCode::TK2631 => "TK2631",
            DiagnosticCode::TK2661 => "TK2661",
            DiagnosticCode::TK2669 => "TK2669",
            DiagnosticCode::TK2670 => "TK2670",
            DiagnosticCode::TK2673 => "TK2673",
            DiagnosticCode::TK2674 => "TK2674",
            DiagnosticCode::TK2684 => "TK2684",
            DiagnosticCode::TK2687 => "TK2687",
            DiagnosticCode::TK2694 => "TK2694",
            DiagnosticCode::TK2702 => "TK2702",
            DiagnosticCode::TK2706 => "TK2706",
            DiagnosticCode::TK2707 => "TK2707",
            DiagnosticCode::TK2713 => "TK2713",
            DiagnosticCode::TK2708 => "TK2708",
            DiagnosticCode::TK2717 => "TK2717",
            DiagnosticCode::TK2540 => "TK2540",
            DiagnosticCode::TK2554 => "TK2554",
            DiagnosticCode::TK2555 => "TK2555",
            DiagnosticCode::TK2558 => "TK2558",
            DiagnosticCode::TK2576 => "TK2576",
            DiagnosticCode::TK2589 => "TK2589",
            DiagnosticCode::TK2741 => "TK2741",
            DiagnosticCode::TK2744 => "TK2744",
            DiagnosticCode::TK2749 => "TK2749",
            DiagnosticCode::TK2769 => "TK2769",
            DiagnosticCode::TK2852 => "TK2852",
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
#[derive(Clone, PartialEq, Eq, Debug)]
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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct QualifiedTypeIncomplete {
    pub id: &'static str,
    pub context: &'static str,
}

pub(crate) fn qualified_type_incomplete(
    resolution: QualifiedTypePathResolution,
) -> Option<QualifiedTypeIncomplete> {
    let (id, context) = match resolution {
        QualifiedTypePathResolution::TypeGroup(_) => (
            "annotation-lower/type-name/qualified-name",
            "qualified type path classified; leaf lowering deferred to WU3",
        ),
        QualifiedTypePathResolution::Deferred {
            reason: QualifiedTypePathDeferredReason::Import,
            ..
        } => (
            "annotation-lower/type-name/qualified-import-alias",
            "qualified import-alias resolution deferred to backlog 15",
        ),
        QualifiedTypePathResolution::Deferred {
            reason: QualifiedTypePathDeferredReason::Enum,
            ..
        } => (
            "annotation-lower/type-name/qualified-enum",
            "qualified enum type resolution deferred to backlog 42",
        ),
        QualifiedTypePathResolution::MissingRoot { .. }
        | QualifiedTypePathResolution::TypeOnlyRoot { .. }
        | QualifiedTypePathResolution::MissingMember { .. }
        | QualifiedTypePathResolution::TypeOnlyIntermediate { .. }
        | QualifiedTypePathResolution::ValueOnlyLeaf { .. }
        | QualifiedTypePathResolution::Unavailable { .. } => return None,
    };
    Some(QualifiedTypeIncomplete { id, context })
}

pub(crate) fn qualified_type_topology_diagnostic(
    resolution: QualifiedTypePathResolution,
    names: &[&str],
    spans: &[Span],
    qualified_span: Span,
) -> Option<Diagnostic> {
    match resolution {
        QualifiedTypePathResolution::MissingRoot { segment } => {
            let name = *names
                .get(segment)
                .expect("binder root failure references a path segment");
            let span = *spans
                .get(segment)
                .expect("binder root failure retains a segment span");
            Some(Diagnostic::cannot_find_namespace(span, name))
        }
        QualifiedTypePathResolution::TypeOnlyRoot { segment } => {
            let name = *names
                .get(segment)
                .expect("binder type-only root references a path segment");
            let span = *spans
                .get(segment)
                .expect("binder type-only root retains a segment span");
            Some(Diagnostic::type_used_as_namespace(span, name))
        }
        QualifiedTypePathResolution::MissingMember { segment } => {
            let member = *names
                .get(segment)
                .expect("binder member failure references a path segment");
            let span = *spans
                .get(segment)
                .expect("binder member failure retains a segment span");
            Some(Diagnostic::namespace_has_no_exported_member(
                span,
                &names[..segment].join("."),
                member,
            ))
        }
        QualifiedTypePathResolution::TypeOnlyIntermediate { segment } => {
            let type_name = *names
                .get(segment)
                .expect("binder intermediate failure references a path segment");
            let property = *names
                .get(segment + 1)
                .expect("type-only intermediate has a following path segment");
            let span = *spans
                .get(segment + 1)
                .expect("type-only intermediate retains the following segment span");
            Some(Diagnostic::cannot_access_type_as_namespace(
                span, type_name, property,
            ))
        }
        QualifiedTypePathResolution::ValueOnlyLeaf { .. } => Some(Diagnostic::value_used_as_type(
            qualified_span,
            &names.join("."),
        )),
        QualifiedTypePathResolution::TypeGroup(_)
        | QualifiedTypePathResolution::Unavailable { .. }
        | QualifiedTypePathResolution::Deferred { .. } => None,
    }
}

impl Diagnostic {
    fn declaration_merge(code: DiagnosticCode, span: Span, message: String) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            message,
            span,
            elaboration: Vec::new(),
        }
    }

    pub fn global_module_export_requires_module(span: Span) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK1314,
            span,
            "Global module exports may only appear in module files.".to_string(),
        )
    }

    pub fn global_module_export_requires_declaration_file(span: Span) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK1315,
            span,
            "Global module exports may only appear in declaration files.".to_string(),
        )
    }

    pub fn global_augmentation_requires_module(span: Span) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2669,
            span,
            "Augmentations for the global scope can only be directly nested in external modules or ambient module declarations.".to_string(),
        )
    }

    pub fn global_augmentation_requires_declare(span: Span) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2670,
            span,
            "Augmentations for the global scope should have 'declare' modifier unless they appear in already ambient context.".to_string(),
        )
    }

    pub fn merged_interface_type_parameters(span: Span, name: &str) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2428,
            span,
            format!("All declarations of '{name}' must have identical type parameters."),
        )
    }

    pub fn duplicate_identifier(span: Span, name: &str) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2300,
            span,
            format!("Duplicate identifier '{name}'."),
        )
    }

    pub fn cannot_redeclare_block_scoped_variable(span: Span, name: &str) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2451,
            span,
            format!("Cannot redeclare block-scoped variable '{name}'."),
        )
    }

    pub fn subsequent_property_type(span: Span, name: &str) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2717,
            span,
            format!("Subsequent property declarations must have the same type. Property '{name}' has a conflicting type."),
        )
    }

    pub fn identical_property_modifiers(span: Span, name: &str) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2687,
            span,
            format!("All declarations of '{name}' must have identical modifiers."),
        )
    }

    pub fn overload_signatures_same_accessibility(span: Span) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2385,
            span,
            "Overload signatures must all be public, private or protected.".to_string(),
        )
    }

    pub fn duplicate_index_signature(span: Span, key: &str) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2374,
            span,
            format!("Duplicate index signature for type '{key}'."),
        )
    }

    pub fn number_index_incompatible(span: Span, source: &str, target: &str) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2413,
            span,
            format!("'number' index type '{source}' is not assignable to 'string' index type '{target}'."),
        )
    }

    pub fn property_incompatible_with_string_index(
        span: Span,
        name: &str,
        source: &str,
        target: &str,
    ) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2411,
            span,
            format!("Property '{name}' of type '{source}' is not assignable to 'string' index type '{target}'."),
        )
    }

    pub fn incompatible_interface_heritage(span: Span, left: &str, right: &str) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2320,
            span,
            format!("Interface cannot simultaneously extend types '{left}' and '{right}'."),
        )
    }

    pub fn incorrectly_extends_interface(span: Span, derived: &str, base: &str) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2430,
            span,
            format!("Interface '{derived}' incorrectly extends interface '{base}'."),
        )
    }

    pub fn namespace_precedes_class_or_function(span: Span) -> Self {
        Self::declaration_merge(
            DiagnosticCode::TK2434,
            span,
            "A namespace declaration cannot be located prior to a class or function with which it is merged".to_string(),
        )
    }

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

    /// Construct a `TK2503` unresolved namespace-root error.
    pub fn cannot_find_namespace(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2503,
            severity: Severity::Error,
            message: format!("Cannot find namespace '{name}'."),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2661` non-local ambient export-alias error.
    pub fn cannot_export_non_local(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2661,
            severity: Severity::Error,
            message: format!(
                "Cannot export '{name}'. Only local declarations can be exported from a module"
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2694` missing public namespace-member error.
    pub fn namespace_has_no_exported_member(span: Span, namespace: &str, member: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2694,
            severity: Severity::Error,
            message: format!("Namespace '{namespace}' has no exported member '{member}'."),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2702` type-only root used as a namespace error.
    pub fn type_used_as_namespace(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2702,
            severity: Severity::Error,
            message: format!(
                "'{name}' only refers to a type, but is being used as a namespace here."
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2713` type-only intermediate path segment error.
    pub fn cannot_access_type_as_namespace(span: Span, type_name: &str, property: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2713,
            severity: Severity::Error,
            message: format!(
                "Cannot access '{type_name}.{property}' because '{type_name}' is a type, but not a namespace. Did you mean to retrieve the type of the property '{property}' in '{type_name}' with '{type_name}[\"{property}\"]'?"
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2749` value-only qualified leaf used as a type error.
    pub fn value_used_as_type(span: Span, path: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2749,
            severity: Severity::Error,
            message: format!(
                "'{path}' refers to a value, but is being used as a type here. Did you mean 'typeof {path}'?"
            ),
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

    /// Construct a `TK2310` cyclic-interface-heritage error at the interface binding.
    pub fn circular_interface_heritage(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2310,
            severity: Severity::Error,
            message: format!("Type '{name}' recursively references itself as a base type."),
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

    /// Construct TK2339 against a declaration's stable source name instead of a
    /// structural renderer fallback.
    pub fn property_does_not_exist_on_named_type(span: Span, name: &str, target: &str) -> Self {
        Self::property_does_not_exist(span, name, target)
    }

    /// Construct TK2339 for the static/value side of a named declaration.
    pub fn property_does_not_exist_on_named_value(span: Span, name: &str, target: &str) -> Self {
        Self::property_does_not_exist(span, name, &format!("typeof {target}"))
    }

    /// Construct a `TK2576` when a published static member is read through an
    /// instance of the same class.
    pub fn static_property_accessed_on_instance(span: Span, name: &str, class: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2576,
            severity: Severity::Error,
            message: format!(
                "Property '{name}' does not exist on type '{class}'. Did you mean to access the static member '{class}.{name}' instead?"
            ),
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

    /// Construct a `TK2684` explicit-`this` receiver error.
    pub fn this_context_not_assignable(span: Span, source: &str, target: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2684,
            severity: Severity::Error,
            message: format!(
                "The 'this' context of type '{source}' is not assignable to method's 'this' of type '{target}'."
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

    /// Construct a `TK2349` for a value whose represented type proves it has no
    /// call signatures.
    pub fn expression_is_not_callable(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2349,
            severity: Severity::Error,
            message: "This expression is not callable".to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

    pub fn expression_is_not_constructable(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2351,
            severity: Severity::Error,
            message: "This expression is not constructable".to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

    pub fn cannot_assign_namespace(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2631,
            severity: Severity::Error,
            message: format!("Cannot assign to '{name}' because it is a namespace"),
            span,
            elaboration: Vec::new(),
        }
    }

    pub fn cannot_use_namespace_as_value(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2708,
            severity: Severity::Error,
            message: format!("Cannot use namespace '{name}' as a value"),
            span,
            elaboration: Vec::new(),
        }
    }

    pub fn ambient_using_not_allowed(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK1545,
            severity: Severity::Error,
            message: "'using' declarations are not allowed in ambient contexts.".to_string(),
            span,
            elaboration: Vec::new(),
        }
    }

    pub fn namespace_await_using_not_allowed(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2852,
            severity: Severity::Error,
            message: "'await using' statements are only allowed within async functions and at the top levels of modules."
                .to_string(),
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

    /// Construct a `TK2314` exact generic type-argument count error.
    pub fn generic_type_requires_arguments(span: Span, display: &str, expected: usize) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2314,
            severity: Severity::Error,
            message: format!("Generic type '{display}' requires {expected} type argument(s)"),
            span,
            elaboration: Vec::new(),
        }
    }

    pub fn type_is_not_generic(span: Span, name: &str) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2315,
            severity: Severity::Error,
            message: format!("Type '{name}' is not generic"),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2707` ranged generic type-argument count error.
    pub fn generic_type_requires_argument_range(
        span: Span,
        display: &str,
        min: usize,
        max: usize,
    ) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2707,
            severity: Severity::Error,
            message: format!(
                "Generic type '{display}' requires between {min} and {max} type arguments"
            ),
            span,
            elaboration: Vec::new(),
        }
    }

    /// Construct a `TK2706` required-after-optional type parameter error.
    pub fn required_type_parameter_after_optional(span: Span) -> Self {
        Diagnostic {
            code: DiagnosticCode::TK2706,
            severity: Severity::Error,
            message: "Required type parameters may not follow optional type parameters."
                .to_string(),
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

#[cfg(test)]
mod qualified_name_tests {
    use super::{qualified_type_incomplete, Diagnostic, DiagnosticCode, QualifiedTypeIncomplete};
    use crate::binder::declaration::TypeGroupId;
    use crate::binder::namespace::{QualifiedTypePathDeferredReason, QualifiedTypePathResolution};
    use crate::span::Span;

    fn assert_diagnostic(diagnostic: Diagnostic, code: DiagnosticCode, message: &str, span: Span) {
        assert_eq!(diagnostic.code, code);
        assert_eq!(diagnostic.message, message);
        assert_eq!(diagnostic.span, span);
    }

    #[test]
    fn qualified_name_diagnostics_match_tsc_6_0_3() {
        assert_diagnostic(
            Diagnostic::cannot_find_namespace(Span::new(1, 5), "Root"),
            DiagnosticCode::TK2503,
            "Cannot find namespace 'Root'.",
            Span::new(1, 5),
        );
        assert_diagnostic(
            Diagnostic::cannot_export_non_local(Span::new(7, 8), "A"),
            DiagnosticCode::TK2661,
            "Cannot export 'A'. Only local declarations can be exported from a module",
            Span::new(7, 8),
        );
        assert_diagnostic(
            Diagnostic::namespace_has_no_exported_member(Span::new(8, 14), "Root.Child", "Hidden"),
            DiagnosticCode::TK2694,
            "Namespace 'Root.Child' has no exported member 'Hidden'.",
            Span::new(8, 14),
        );
        assert_diagnostic(
            Diagnostic::type_used_as_namespace(Span::new(2, 6), "OnlyType"),
            DiagnosticCode::TK2702,
            "'OnlyType' only refers to a type, but is being used as a namespace here.",
            Span::new(2, 6),
        );
        assert_diagnostic(
            Diagnostic::cannot_access_type_as_namespace(
                Span::new(18, 22),
                "Middle",
                "Leaf",
            ),
            DiagnosticCode::TK2713,
            "Cannot access 'Middle.Leaf' because 'Middle' is a type, but not a namespace. Did you mean to retrieve the type of the property 'Leaf' in 'Middle' with 'Middle[\"Leaf\"]'?",
            Span::new(18, 22),
        );
        assert_diagnostic(
            Diagnostic::value_used_as_type(Span::new(4, 20), "Root.Value"),
            DiagnosticCode::TK2749,
            "'Root.Value' refers to a value, but is being used as a type here. Did you mean 'typeof Root.Value'?",
            Span::new(4, 20),
        );
    }

    #[test]
    fn qualified_name_codes_render_with_tk_prefix() {
        assert_eq!(DiagnosticCode::TK2503.as_str(), "TK2503");
        assert_eq!(DiagnosticCode::TK2661.as_str(), "TK2661");
        assert_eq!(DiagnosticCode::TK2694.as_str(), "TK2694");
        assert_eq!(DiagnosticCode::TK2702.as_str(), "TK2702");
        assert_eq!(DiagnosticCode::TK2713.as_str(), "TK2713");
        assert_eq!(DiagnosticCode::TK2749.as_str(), "TK2749");
    }

    #[test]
    fn namespace_placement_diagnostic_matches_tsc() {
        let span = Span::new(3, 12);
        assert_diagnostic(
            Diagnostic::namespace_precedes_class_or_function(span),
            DiagnosticCode::TK2434,
            "A namespace declaration cannot be located prior to a class or function with which it is merged",
            span,
        );
        assert_eq!(DiagnosticCode::TK2434.as_str(), "TK2434");
    }

    #[test]
    fn block_scoped_redeclaration_diagnostic_matches_tsc() {
        let span = Span::new(5, 10);
        assert_diagnostic(
            Diagnostic::cannot_redeclare_block_scoped_variable(span, "value"),
            DiagnosticCode::TK2451,
            "Cannot redeclare block-scoped variable 'value'.",
            span,
        );
        assert_eq!(DiagnosticCode::TK2451.as_str(), "TK2451");
    }

    #[test]
    fn overload_accessibility_diagnostic_matches_tsc() {
        let span = Span::new(13, 27);
        assert_diagnostic(
            Diagnostic::overload_signatures_same_accessibility(span),
            DiagnosticCode::TK2385,
            "Overload signatures must all be public, private or protected.",
            span,
        );
        assert_eq!(DiagnosticCode::TK2385.as_str(), "TK2385");
    }

    #[test]
    fn qualified_incomplete_reasons_keep_their_backlog_owner() {
        let cases = [
            (
                QualifiedTypePathResolution::TypeGroup(TypeGroupId(1)),
                QualifiedTypeIncomplete {
                    id: "annotation-lower/type-name/qualified-name",
                    context: "qualified type path classified; leaf lowering deferred to WU3",
                },
            ),
            (
                QualifiedTypePathResolution::Deferred {
                    segment: 0,
                    reason: QualifiedTypePathDeferredReason::Import,
                },
                QualifiedTypeIncomplete {
                    id: "annotation-lower/type-name/qualified-import-alias",
                    context: "qualified import-alias resolution deferred to backlog 15",
                },
            ),
            (
                QualifiedTypePathResolution::Deferred {
                    segment: 1,
                    reason: QualifiedTypePathDeferredReason::Enum,
                },
                QualifiedTypeIncomplete {
                    id: "annotation-lower/type-name/qualified-enum",
                    context: "qualified enum type resolution deferred to backlog 42",
                },
            ),
        ];
        for (resolution, expected) in cases {
            assert_eq!(qualified_type_incomplete(resolution), Some(expected));
        }
        assert_eq!(
            qualified_type_incomplete(QualifiedTypePathResolution::Unavailable { segment: 1 }),
            None
        );
    }
}
