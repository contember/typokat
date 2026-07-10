//! Class member-body checking and access-control lookups (extracted from classes.rs).

use super::super::context::*;
use super::super::decls::value_decl_id;
use crate::binder::scope::ScopeId;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{ClassId, Visibility};
use crate::types::store::TypeId;
use oxc_ast::ast::{Class, ClassElement, MethodDefinitionKind, ObjectPattern};
use oxc_span::GetSpan;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Check class member bodies after type-only class lowering has completed.
    ///
    /// Per-class context is save/restored so `this`, `super`, access-control context,
    /// constructor-only readonly writes, and class type parameters do not leak.
    pub(in crate::check::checker) fn check_class(&mut self, scope: ScopeId, class: &Class<'_>) {
        // The class's `new`-info, looked up via its value slot. Absent only for an
        // anonymous class (out of subset) or an unrecognized declaration — then `this`
        // stays whatever it was (a nested class inside a method keeps the outer `this`,
        // which is the closest plan-faithful behaviour for the deferred nested-class case).
        let info = class
            .id
            .as_ref()
            .and_then(|id| value_decl_id(self.binder, scope, id.name.as_str()))
            .and_then(|decl_id| self.class_ctors.get(&decl_id))
            .copied();

        // M16: the class's value `DeclId`, used to look up its type parameters so a member
        // **body** annotation referencing `T` (a parameter / return type) resolves while the
        // body is checked — exactly as the instance template was lowered in `fill_class`.
        let value_decl = class
            .id
            .as_ref()
            .and_then(|id| value_decl_id(self.binder, scope, id.name.as_str()));

        // Save class context so nested classes do not permanently clobber enclosing state.
        let saved_this = self.current_this;
        let saved_class = self.current_class;
        let saved_super_ctor = self.current_super_ctor;
        // A nested class inside a constructor must not inherit constructor-only writes.
        let saved_in_ctor = self.current_in_ctor;
        self.current_in_ctor = false;
        if let Some(info) = info {
            self.current_this = Some(info.instance);
            // M13: the accessing context for access control is this class's `ClassId`, so
            // `this.privateMember` / a same-class `other.privateMember` resolve, and an
            // inherited `protected` member is reachable via the subclass walk.
            self.current_class = Some(info.class_id);
            // `super_ctor` is itself `Option`: a derived class's base ctor, or `None` for
            // a class with no `extends` (then `super(...)` inside has no signature).
            self.current_super_ctor = info.super_ctor;
        }

        // M13: the **static side** type — the `this` inside a `static` member body /
        // static field initializer (where `this` is the class value, not an instance).
        // `None` when the class is unrecognized (then a static body keeps the enclosing
        // `this`, the same defensive choice as for instance bodies above).
        let static_this = info.map(|info| info.static_side);

        // Rebuild the class type-parameter frame so body annotations resolve to template ids.
        let type_params = value_decl
            .and_then(|decl_id| self.class_type_params.get(&decl_id).cloned())
            .unwrap_or_default();
        let frame = self.build_type_param_frame(class.type_parameters.as_deref(), &type_params);

        // Member bodies share the class type-parameter frame.
        self.with_type_params(frame, |pass| {
            pass.check_class_member_bodies(scope, class, static_this)
        });

        self.current_this = saved_this;
        self.current_class = saved_class;
        self.current_super_ctor = saved_super_ctor;
        self.current_in_ctor = saved_in_ctor;
    }

    /// Check member bodies with class context already bound by [`check_class`].
    ///
    /// Per-member `this` and constructor-readonly state are save/restored so static
    /// and instance bodies cannot leak context into each other.
    fn check_class_member_bodies(
        &mut self,
        scope: ScopeId,
        class: &Class<'_>,
        static_this: Option<TypeId>,
    ) {
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                // Field initializers are checked here, with `this` bound to the instance
                // or static side. Static blocks and `accessor` properties remain out of
                // subset; `get`/`set` accessors are handled as methods below.
                if let ClassElement::PropertyDefinition(prop) = element {
                    if let Some(init) = &prop.value {
                        // A static initializer's `this` is the class value; an instance
                        // initializer's is the instance. `current_class` is unchanged
                        // (same-class access is allowed in both).
                        let saved_member_this = self.current_this;
                        if prop.r#static {
                            if let Some(static_this) = static_this {
                                self.current_this = Some(static_this);
                            }
                        }
                        // Annotated field initializers use the variable-initializer path
                        // (assignability, fresh excess, contextual typing). `readonly` is
                        // irrelevant here; optional fields target their `T | undefined` type.
                        let annotation = prop
                            .type_annotation
                            .as_ref()
                            .and_then(|ann| self.lower_annotation(scope, &ann.type_annotation))
                            .map(|ty| {
                                if prop.optional {
                                    self.optional_field_effective_type(ty)
                                } else {
                                    ty
                                }
                            });
                        self.check_annotated_initializer(scope, annotation, init);
                        self.current_this = saved_member_this;
                    }
                }
                continue;
            };
            // Use the shared function machine for body-checking side effects; the
            // property type was already built in `fill_class`.

            // Static methods bind `this` to the static side while keeping
            // `current_class` for same-class access. Save/restore prevents leaks to the
            // next member.
            if method.r#static {
                let saved_member_this = self.current_this;
                if let Some(static_this) = static_this {
                    self.current_this = Some(static_this);
                }
                let _ = self.infer_function(scope, &method.value);
                self.current_this = saved_member_this;
            } else {
                // M14: mark the constructor body so a `this.readonly = …` assignment is
                // allowed there (and only there). A constructor is never `static`; every
                // other (instance) method body keeps `current_in_ctor == false`. Saved/
                // restored per member so it does not leak to a following method.
                let saved_in_ctor = self.current_in_ctor;
                self.current_in_ctor = matches!(method.kind, MethodDefinitionKind::Constructor);
                let _ = self.infer_function(scope, &method.value);
                self.current_in_ctor = saved_in_ctor;
            }
        }
    }

    /// Apply M13 access control to a found member access.
    /// `private` requires the declaring class as current context; `protected`
    /// also allows subclasses. The rule keys on the member's declaring class, not
    /// the instance, so same-class access to another instance's private member works.
    pub(in crate::check::checker) fn check_member_access_control(
        &mut self,
        prop_name: &str,
        prop_span: Span,
        visibility: Visibility,
        declaring_class: Option<ClassId>,
    ) {
        match visibility {
            Visibility::Public => {}
            Visibility::Private => {
                // Reachable only inside the exact declaring class's body.
                let allowed = matches!(
                    (self.current_class, declaring_class),
                    (Some(ctx), Some(owner)) if ctx == owner
                );
                if !allowed {
                    self.diagnostics
                        .push(Diagnostic::property_is_private(prop_span, prop_name));
                }
            }
            Visibility::Protected => {
                // Reachable inside the declaring class or any subclass of it.
                let allowed = match (self.current_class, declaring_class) {
                    (Some(ctx), Some(owner)) => self.is_class_or_subclass(ctx, owner),
                    _ => false,
                };
                if !allowed {
                    self.diagnostics
                        .push(Diagnostic::property_is_protected(prop_span, prop_name));
                }
            }
        }
    }

    /// Run M13 access control for named object-destructuring keys.
    /// Renames check the source key, and the current class context mirrors `obj.key`.
    /// Missing properties stay silent (`TK2339` for destructuring is deferred).
    /// Rest, computed keys, and nested/default/array patterns are skipped.
    pub(in crate::check::checker) fn check_object_pattern_access(
        &mut self,
        pattern: &ObjectPattern<'_>,
        source: TypeId,
    ) {
        for property in &pattern.properties {
            // The checked name is the pattern KEY (`{ priv: a }` checks `priv`). A
            // computed / non-static key yields `None` and is skipped.
            let Some(name) = property.key.static_name() else {
                continue;
            };
            // Resolve the member's visibility + origin on the source type, then run the
            // shared access check. `None` = not found → stay silent (no TK2339 here).
            if let Some((visibility, declaring_class)) = self.pattern_member_access(source, &name) {
                let span = Span::from_oxc(property.key.span());
                self.check_member_access_control(&name, span, visibility, declaring_class);
            }
        }
    }

    /// Look up a destructured member's access-control metadata.
    /// Objects are direct; unions require the member on every constituent. Missing
    /// means "no check" per the destructuring `TK2339` deferral, and `any`/error
    /// constituents contribute no nominal origin.
    fn pattern_member_access(
        &self,
        source: TypeId,
        name: &str,
    ) -> Option<(Visibility, Option<ClassId>)> {
        let store = self.interner.store();
        // M24 (audit): a constrained-type-parameter source (`function f<T extends K>({
        // priv }: T)`) resolves through its apparent type, like every other structural
        // consumer. Identity for a non-parameter source.
        let source = self.apparent_type(source);
        // M31: an intersection property is present if any object member declares it;
        // keep private/protected origins so access remains gated.
        if let Some(members) = store.intersection_members(source) {
            let mut result: Option<(Visibility, Option<ClassId>)> = None;
            for &member in members {
                let member = self.apparent_type(member);
                if let Some(prop) = store.object_type(member).and_then(|o| o.property(name)) {
                    if result.is_none()
                        || matches!(prop.visibility, Visibility::Private | Visibility::Protected)
                    {
                        result = Some((prop.visibility, prop.declaring_class));
                    }
                }
            }
            return result;
        }
        if let Some(members) = store.union_members(source) {
            // Require the property on every constituent; report the first non-public
            // origin found so the union still gates a private/protected member.
            let wk = self.interner.well_known();
            let mut result: Option<(Visibility, Option<ClassId>)> = None;
            for &member in members {
                if member == wk.any || member == wk.error {
                    continue;
                }
                // A constrained-param constituent resolves through its apparent type too.
                let member = self.apparent_type(member);
                match store.object_type(member).and_then(|o| o.property(name)) {
                    Some(prop) => {
                        if result.is_none()
                            || matches!(
                                prop.visibility,
                                Visibility::Private | Visibility::Protected
                            )
                        {
                            result = Some((prop.visibility, prop.declaring_class));
                        }
                    }
                    // Missing on this constituent → not on the union.
                    None => return None,
                }
            }
            return result;
        }
        store
            .object_type(source)
            .and_then(|obj| obj.property(name))
            .map(|prop| (prop.visibility, prop.declaring_class))
    }
}
