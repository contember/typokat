//! Constructor accessibility and visibility-modifier lowering (extracted from classes.rs).

use super::super::context::*;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::Visibility;
use oxc_ast::ast::{Class, ClassElement, MethodDefinitionKind, TSAccessibility};

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    /// Gate direct `new C(...)` on constructor accessibility.
    /// Emits `TK2673`/`TK2674`, returns whether to suppress `TK2511`, and checks
    /// private/protected reachability against the constructor's declaring class.
    pub(in crate::check::checker) fn check_new_accessibility(
        &mut self,
        info: &ClassInfo,
        new_span: Span,
    ) -> bool {
        let declaring = info.ctor_declaring_class;
        let diag = match info.ctor_visibility {
            // A public (explicit or implicit) constructor is always accessible.
            Visibility::Public => return false,
            // Private: reachable only inside the exact declaring class's body.
            Visibility::Private => {
                if self.has_exact_class_access_context(declaring) {
                    return false;
                }
                let name = self.declaring_class_name(declaring);
                Diagnostic::constructor_is_private(new_span, name)
            }
            // Protected: reachable inside the declaring class or any subclass of it.
            Visibility::Protected => {
                let allowed = self.has_derived_class_access_context(declaring);
                if allowed {
                    return false;
                }
                let name = self.declaring_class_name(declaring);
                Diagnostic::constructor_is_protected(new_span, name)
            }
        };
        self.emit_diagnostic(diag);
        true
    }
}

/// Lower an AST `accessibility` modifier ([`TSAccessibility`]) to a [`Visibility`]
/// (M13). An absent modifier (`None`) is `public` (the default — no diagnostics).
pub(super) fn lower_visibility(accessibility: Option<TSAccessibility>) -> Visibility {
    match accessibility {
        Some(TSAccessibility::Private) => Visibility::Private,
        Some(TSAccessibility::Protected) => Visibility::Protected,
        Some(TSAccessibility::Public) | None => Visibility::Public,
    }
}

/// Whether the class's static side should expose a public construct signature. A
/// class with no explicit constructor has the implicit public constructor; an
/// explicit `private`/`protected` constructor is not publicly constructable.
pub(super) fn has_public_constructor(class: &Class<'_>) -> bool {
    for element in &class.body.body {
        let ClassElement::MethodDefinition(method) = element else {
            continue;
        };
        if method.kind != MethodDefinitionKind::Constructor {
            continue;
        }
        return lower_visibility(method.accessibility) == Visibility::Public;
    }
    true
}

/// The visibility of the class's **own** explicit constructor (backlog 20). A class
/// with no explicit constructor has the implicit **public** constructor. Callers use
/// this only when an own constructor exists (`ctor_params.is_some()`); a class with no
/// own constructor inherits the base's visibility instead of calling this.
pub(in crate::check::checker) fn constructor_visibility(class: &Class<'_>) -> Visibility {
    for element in &class.body.body {
        let ClassElement::MethodDefinition(method) = element else {
            continue;
        };
        if method.kind != MethodDefinitionKind::Constructor {
            continue;
        }
        return lower_visibility(method.accessibility);
    }
    Visibility::Public
}

pub(in crate::check::checker) fn constructor_declaration_count(class: &Class<'_>) -> usize {
    class
        .body
        .body
        .iter()
        .filter(|element| {
            matches!(
                element,
                ClassElement::MethodDefinition(method)
                    if method.kind == MethodDefinitionKind::Constructor
            )
        })
        .count()
}
