//! Class inheritance: subclass walks, override + abstract-completeness checks
//! (extracted from classes.rs).

use super::super::context::*;
use super::*;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{ClassId, PropertyType, Visibility};
use oxc_ast::ast::{Class, ClassElement, MethodDefinitionKind};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// The display name of a constructor's declaring class (backlog 20), for a
    /// `TK2673`/`TK2674` message. A defensive empty string keeps the message well-formed
    /// if the id is somehow unknown (every named class is recorded in [`fill_class`]).
    pub(super) fn declaring_class_name(&self, class_id: ClassId) -> &str {
        self.class_names
            .get(&class_id)
            .map(String::as_str)
            .unwrap_or("")
    }

    /// Whether `class_id` is `ancestor` or a subclass of it.
    /// Used for `protected` access; a visited set keeps malformed `extends` cycles
    /// from hanging.
    pub(super) fn is_class_or_subclass(&self, class_id: ClassId, ancestor: ClassId) -> bool {
        let mut current = class_id;
        let mut visited: rustc_hash::FxHashSet<ClassId> = rustc_hash::FxHashSet::default();
        loop {
            if current == ancestor {
                return true;
            }
            if !visited.insert(current) {
                // Re-entered a class already seen — a cyclic hierarchy; stop.
                return false;
            }
            match self.class_parents.get(&current).copied() {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }

    /// Record phase-2 `TK2416` checks for public own members overriding public base members.
    /// Effective types are related later, with method bivariance keyed on the base
    /// member kind. Private/protected overrides, error-typed members, spanless members,
    /// and generic/free-parameter cases are deferred or skipped.
    pub(super) fn collect_override_checks(
        &mut self,
        class: &Class<'_>,
        own_instance: &[PropertyType],
        base_instance: &[PropertyType],
        base_member_kinds: &FxHashMap<String, bool>,
    ) {
        let wk = self.interner.well_known();
        let Some(derived_name) = class.id.as_ref().map(|id| id.name.as_str()) else {
            return;
        };
        let Some(base_name) = base_class_name(class) else {
            return;
        };
        let spans = own_instance_member_spans(class);
        for own in own_instance {
            if own.visibility != Visibility::Public {
                continue;
            }
            let Some(base_member) = base_instance.iter().find(|p| p.name == own.name) else {
                continue;
            };
            if base_member.visibility != Visibility::Public {
                continue;
            }
            if own.ty == wk.error || base_member.ty == wk.error {
                continue;
            }
            let Some(&span) = spans.get(&own.name) else {
                continue;
            };
            let base_is_method = base_member_kinds.get(&own.name).copied().unwrap_or(false);
            self.override_checks.push(OverrideCheck {
                own_ty: own.ty,
                base_ty: base_member.ty,
                name: own.name.clone(),
                derived: derived_name.to_string(),
                base: base_name.to_string(),
                span,
                base_is_method,
            });
        }
    }

    /// Emit abstract-member-completeness diagnostics for non-abstract classes.
    /// One missing member is `TK2515`; multiple are aggregated as `TK2654`.
    /// Requires a named class and resolvable direct base for attribution.
    pub(super) fn report_missing_abstract_members(
        &mut self,
        class: &Class<'_>,
        pending: &[String],
    ) {
        let Some(id) = class.id.as_ref() else {
            return;
        };
        let Some(base_name) = base_class_name(class) else {
            return;
        };
        let span = Span::from_oxc(id.span);
        let class_name = id.name.as_str();
        match pending {
            [] => {}
            [one] => self.diagnostics.push(Diagnostic::missing_abstract_member(
                span, class_name, one, base_name,
            )),
            many => self.diagnostics.push(Diagnostic::missing_abstract_members(
                span, class_name, many, base_name,
            )),
        }
    }
}

/// Name spans for own instance members, used to position `TK2416`.
/// The first span wins for duplicate names; constructor parameter properties are
/// absent because their override check is deferred.
fn own_instance_member_spans(class: &Class<'_>) -> FxHashMap<String, Span> {
    let mut spans: FxHashMap<String, Span> = FxHashMap::default();
    for element in &class.body.body {
        match element {
            ClassElement::PropertyDefinition(prop) => {
                if prop.computed || prop.r#static {
                    continue;
                }
                if let Some(name) = prop.key.static_name() {
                    spans
                        .entry(name.into_owned())
                        .or_insert_with(|| Span::from_oxc(prop.key.span()));
                }
            }
            ClassElement::MethodDefinition(method) => {
                if method.computed
                    || method.r#static
                    || matches!(method.kind, MethodDefinitionKind::Constructor)
                {
                    continue;
                }
                if let Some(name) = method.key.static_name() {
                    spans
                        .entry(name.into_owned())
                        .or_insert_with(|| Span::from_oxc(method.key.span()));
                }
            }
            _ => {}
        }
    }
    spans
}
