//! Class member-body checking and access-control lookups (extracted from classes.rs).

use super::super::calls::RetainedFunctionBodySurface;
use super::super::context::*;
use super::application::build_open_class_application;
use super::body::{BodyClassView, BodyMemberLookup};
use super::retained::RetainedClassCallable;
use super::surface_types::SurfaceTypeFactory;
use crate::binder::scope::ScopeId;
use crate::check::checker::lexical_events::LexicalOwnerPhase;
use crate::check::query::SemanticQueryCoordinator;
use crate::class_semantics::DemandOutcome;
use crate::diagnostics::Diagnostic;
use crate::relate::RelationOutcome;
use crate::span::Span;
use crate::types::repr::{ClassId, TypeParamId, Visibility};
use crate::types::store::TypeId;
use oxc_ast::ast::{Class, ClassElement, MethodDefinitionKind, ObjectPattern};
use oxc_span::GetSpan;

struct ClassBodySurfaces {
    instance: Option<TypeId>,
    static_side: Option<TypeId>,
    member_view: Option<BodyClassView>,
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    /// Check class member bodies after type-only class lowering has completed.
    ///
    /// Per-class context is save/restored so `this`, `super`, access-control context,
    /// constructor-only readonly writes, and class type parameters do not leak.
    pub(in crate::check::checker) fn check_class(&mut self, scope: ScopeId, class: &Class<'_>) {
        // The class's `new`-info, looked up via its value slot. Absent only for an
        // anonymous class (out of subset) or an unrecognized declaration — then `this`
        // stays whatever it was (a nested class inside a method keeps the outer `this`,
        // which is the closest plan-faithful behaviour for the deferred nested-class case).
        let binding = self
            .lexical_events
            .class_at(
                super::super::lexical_events::source_ordinal(self.current_source),
                class.span.start,
            )
            .and_then(|site| self.lexical_events.class(site))
            .and_then(|reservation| reservation.binding.as_ref())
            .cloned();
        let published_templates = binding.as_ref().and_then(|binding| {
            match self
                .type_environment
                .published()
                .classes()
                .published_class(binding.class_id)
            {
                DemandOutcome::Ready(surface) => {
                    Some((surface.instance_template(), surface.static_template()))
                }
                DemandOutcome::Exhausted(_) => None,
            }
        });
        let body_view = binding
            .as_ref()
            .and_then(|binding| self.class_body_views.get(&binding.class_id).cloned());
        let retained = binding
            .as_ref()
            .and_then(|binding| self.retained_class_callables.get(&binding.class_id))
            .cloned()
            .unwrap_or_default();

        // M16: the class's value-storage id, used to look up its type parameters so a member
        // **body** annotation referencing `T` (a parameter / return type) resolves while the
        // body is checked — exactly as the instance template was lowered in `fill_class`.
        // Save class context so nested classes do not permanently clobber enclosing state.
        let saved_this = self.current_this;
        let saved_body_this_environment = self.current_body_this_environment.clone();
        let saved_class = self.current_class;
        let saved_super_ctor = self.current_super_ctor;
        // A nested class inside a constructor must not inherit constructor-only writes.
        let saved_in_ctor = self.current_in_ctor;
        self.current_in_ctor = false;
        self.current_body_this_environment = None;
        let enclosing_class = binding.as_ref().and(saved_class);
        if let Some(enclosing_class) = enclosing_class {
            self.enclosing_classes.push(enclosing_class);
        }
        let descriptors = binding
            .as_ref()
            .and_then(|binding| self.class_application_parameters.get(&binding.class_id))
            .cloned()
            .unwrap_or_default();
        let application_type_params: Vec<TypeParamId> = descriptors
            .iter()
            .map(|descriptor| descriptor.application().id)
            .collect();
        let class_type_params = binding
            .as_ref()
            .map(|binding| binding.header_type_params.clone())
            .unwrap_or_default();
        let recovery_names = binding
            .as_ref()
            .and_then(|binding| {
                self.type_environment
                    .published()
                    .groups()
                    .get(binding.type_decl)
            })
            .and_then(|terminal| match terminal {
                super::super::type_groups::PublishedTypeGroupTerminal::Ready(group) => {
                    Some(group.parameter_names.clone())
                }
                super::super::type_groups::PublishedTypeGroupTerminal::Unavailable(_) => None,
            })
            .unwrap_or_default();
        let parameter_types: Vec<TypeId> = application_type_params
            .iter()
            .copied()
            .zip(recovery_names.iter())
            .map(|(id, name)| self.interner.intern_type_param(id, name))
            .collect();
        if let Some(binding) = binding.as_ref() {
            self.current_class = Some(binding.class_id);
            self.current_super_ctor = self
                .class_super_constructors
                .get(&binding.class_id)
                .copied();
            let parameters = descriptors
                .iter()
                .map(|descriptor| *descriptor.application())
                .collect::<Vec<_>>();
            if published_templates.is_some() {
                let current_this = build_open_class_application(
                    &mut SurfaceTypeFactory::new(self.interner),
                    binding.class_id,
                    &parameters,
                    &parameter_types,
                );
                if let DemandOutcome::Ready(current_this) = current_this {
                    self.current_this = Some(current_this);
                }
            } else {
                self.current_this = None;
            }
        }

        // M13: the **static side** type — the `this` inside a `static` member body /
        // static field initializer (where `this` is the class value, not an instance).
        // `None` when the class is unrecognized (then a static body keeps the enclosing
        // `this`, the same defensive choice as for instance bodies above).
        let static_this = published_templates.map(|(_, static_template)| static_template);
        let body_member_view = published_templates.is_none().then_some(body_view).flatten();

        // Rebuild the class type-parameter frame so body annotations resolve to template ids.
        let frame =
            self.build_type_param_frame(class.type_parameters.as_deref(), &class_type_params);

        // Member bodies share the class type-parameter frame.
        self.with_type_params(frame, |pass| {
            pass.check_class_member_bodies(
                scope,
                class,
                ClassBodySurfaces {
                    instance: published_templates.map(|(instance_template, _)| instance_template),
                    static_side: static_this,
                    member_view: body_member_view,
                },
                &class_type_params,
                &retained,
            )
        });

        self.current_this = saved_this;
        self.current_body_this_environment = saved_body_this_environment;
        self.current_class = saved_class;
        if let Some(enclosing_class) = enclosing_class {
            let restored = self.enclosing_classes.pop();
            debug_assert_eq!(restored, Some(enclosing_class));
        }
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
        surfaces: ClassBodySurfaces,
        class_type_params: &[TypeParamId],
        retained: &[RetainedClassCallable<Ticket>],
    ) {
        let ClassBodySurfaces {
            instance: instance_surface,
            static_side: static_this,
            member_view: body_member_view,
        } = surfaces;
        self.check_class_overload_implementations(class, retained);
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
                        let saved_body_this_environment =
                            self.current_body_this_environment.clone();
                        self.current_body_this_environment =
                            body_member_view.as_ref().map(|view| {
                                if prop.r#static {
                                    view.static_side.clone()
                                } else {
                                    view.instance.clone()
                                }
                            });
                        if prop.r#static {
                            if let Some(static_this) = static_this {
                                self.current_this = Some(static_this);
                            }
                        }
                        // Annotated field initializers use the variable-initializer path
                        // (assignability, fresh excess, contextual typing). `readonly` is
                        // irrelevant here; optional fields target their `T | undefined` type.
                        self.with_lexical_effects(
                            prop.span.start,
                            LexicalOwnerPhase::Body,
                            |pass| {
                                let owner_surface = if prop.r#static {
                                    static_this
                                } else {
                                    instance_surface
                                };
                                let annotation = prop
                                    .type_annotation
                                    .as_ref()
                                    .and_then(|_| prop.key.static_name())
                                    .and_then(|name| {
                                        owner_surface
                                            .and_then(|surface| {
                                                pass.interner.store().object_type(surface)
                                            })
                                            .and_then(|object| object.property(&name))
                                            .map(|property| property.ty)
                                            .or_else(|| pass.current_body_member_type(&name))
                                    });
                                pass.check_annotated_initializer(scope, annotation, init);
                            },
                        );
                        self.current_this = saved_member_this;
                        self.current_body_this_environment = saved_body_this_environment;
                    }
                }
                continue;
            };
            // Use the shared function machine for body-checking side effects; the
            // property type was already built in `fill_class`.
            if method.computed {
                if let Some(key) = method.key.as_expression() {
                    self.with_lexical_effects(
                        method.span.start,
                        LexicalOwnerPhase::Immediate,
                        |pass| {
                            pass.infer_expr(scope, key);
                        },
                    );
                }
            }

            // Static methods bind `this` to the static side while keeping
            // `current_class` for same-class access. Save/restore prevents leaks to the
            // next member.
            if method.r#static {
                let saved_member_this = self.current_this;
                let saved_body_this_environment = self.current_body_this_environment.clone();
                self.current_body_this_environment = body_member_view
                    .as_ref()
                    .map(|view| view.static_side.clone());
                if let Some(static_this) = static_this {
                    self.current_this = Some(static_this);
                }
                self.with_static_class_type_param_barrier(class_type_params, |pass| {
                    pass.fill_retained_class_callable(scope, &method.value, retained);
                });
                self.current_this = saved_member_this;
                self.current_body_this_environment = saved_body_this_environment;
            } else {
                // M14: mark the constructor body so a `this.readonly = …` assignment is
                // allowed there (and only there). A constructor is never `static`; every
                // other (instance) method body keeps `current_in_ctor == false`. Saved/
                // restored per member so it does not leak to a following method.
                let saved_in_ctor = self.current_in_ctor;
                let saved_body_this_environment = self.current_body_this_environment.clone();
                self.current_body_this_environment =
                    body_member_view.as_ref().map(|view| view.instance.clone());
                self.current_in_ctor = matches!(method.kind, MethodDefinitionKind::Constructor);
                self.fill_retained_class_callable(scope, &method.value, retained);
                self.current_in_ctor = saved_in_ctor;
                self.current_body_this_environment = saved_body_this_environment;
            }
        }
    }

    pub(in crate::check::checker) fn body_this_member_lookup(
        &self,
        expression: &oxc_ast::ast::Expression<'_>,
        name: &str,
    ) -> Option<BodyMemberLookup> {
        if !body_base_is_this(expression) {
            return None;
        }
        self.current_body_this_environment
            .as_ref()
            .map(|environment| environment.lookup(name))
    }

    fn current_body_member_type(&self, name: &str) -> Option<TypeId> {
        match self.current_body_this_environment.as_ref()?.lookup(name) {
            BodyMemberLookup::Known { ty, .. } => Some(ty),
            BodyMemberLookup::Unavailable(_) | BodyMemberLookup::Missing { .. } => None,
        }
    }

    fn fill_retained_class_callable(
        &mut self,
        scope: ScopeId,
        function: &oxc_ast::ast::Function<'_>,
        retained: &[RetainedClassCallable<Ticket>],
    ) {
        let retained = self
            .lexical_events
            .callable_at(
                super::super::lexical_events::source_ordinal(self.current_source),
                function.span.start,
            )
            .and_then(|site| {
                retained
                    .iter()
                    .find(|retained| retained.site == site)
                    .cloned()
            });
        let Some(retained) = retained else {
            return;
        };
        let Some(function_ty) = retained.public_type else {
            self.check_retained_function_body(
                scope,
                function,
                &RetainedFunctionBodySurface {
                    type_param_frame: retained.type_param_frame,
                    receiver: retained.receiver,
                    params: retained.params,
                    declared_return: retained.declared_return,
                    tickets: Some(retained.tickets),
                },
            );
            return;
        };
        let surface = FunctionSurface {
            receiver: retained.receiver,
            params: retained
                .params
                .into_iter()
                .map(|parameter| {
                    parameter.expect("published callable retains every source parameter")
                })
                .collect(),
            generic_params: retained.type_params,
            type_param_frame: retained.type_param_frame,
            declared_return: retained.declared_return,
            function_ty,
            tickets: Some(retained.tickets),
        };
        self.fill_reserved_function(scope, function, &surface);
    }

    fn check_class_overload_implementations(
        &mut self,
        class: &Class<'_>,
        retained: &[RetainedClassCallable<Ticket>],
    ) {
        let mut index = 0;
        while index < class.body.body.len() {
            let ClassElement::MethodDefinition(first) = &class.body.body[index] else {
                index += 1;
                continue;
            };
            let Some(name) = first.key.static_name() else {
                index += 1;
                continue;
            };
            let mut end = index + 1;
            while end < class.body.body.len() {
                let ClassElement::MethodDefinition(next) = &class.body.body[end] else {
                    break;
                };
                if next.key.static_name().as_deref() != Some(name.as_ref()) {
                    break;
                }
                end += 1;
            }
            let group = &class.body.body[index..end];
            let implementation = group.iter().rev().find_map(|element| {
                let ClassElement::MethodDefinition(method) = element else {
                    return None;
                };
                method.value.body.as_ref()?;
                self.retained_callable_type(&method.value, retained)
            });
            if let Some(implementation) = implementation {
                for element in group {
                    let ClassElement::MethodDefinition(signature) = element else {
                        continue;
                    };
                    if signature.value.body.is_some() {
                        continue;
                    }
                    let Some(signature_ty) =
                        self.retained_callable_type(&signature.value, retained)
                    else {
                        continue;
                    };
                    let outcome = SemanticQueryCoordinator::new(
                        self.interner,
                        self.type_environment.published().classes(),
                        &mut self.semantic_queries,
                        &mut self.next_type_param,
                    )
                    .overload_implementation_compatible(signature_ty, implementation);
                    if matches!(outcome, RelationOutcome::No(_)) {
                        self.with_lexical_effects(
                            signature.span.start,
                            LexicalOwnerPhase::Deferred,
                            |pass| {
                                pass.emit_diagnostic(Diagnostic::overload_incompatible(
                                    Span::from_oxc(signature.key.span()),
                                ));
                            },
                        );
                        break;
                    }
                    if let RelationOutcome::Exhausted(exhaustion) = outcome {
                        self.with_lexical_effects(
                            signature.span.start,
                            LexicalOwnerPhase::Deferred,
                            |pass| {
                                pass.own_type_demand(
                                    DemandOutcome::Exhausted(exhaustion),
                                    Span::from_oxc(signature.key.span()),
                                );
                            },
                        );
                        break;
                    }
                }
            }
            index = end;
        }
    }

    fn retained_callable_type(
        &self,
        function: &oxc_ast::ast::Function<'_>,
        retained: &[RetainedClassCallable<Ticket>],
    ) -> Option<TypeId> {
        let site = self.lexical_events.callable_at(
            super::super::lexical_events::source_ordinal(self.current_source),
            function.span.start,
        )?;
        retained
            .iter()
            .find(|retained| retained.site == site)
            .and_then(|retained| retained.public_type)
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
                let allowed =
                    declaring_class.is_some_and(|owner| self.has_exact_class_access_context(owner));
                if !allowed {
                    self.emit_diagnostic(Diagnostic::property_is_private(prop_span, prop_name));
                }
            }
            Visibility::Protected => {
                let allowed = declaring_class
                    .is_some_and(|owner| self.has_derived_class_access_context(owner));
                if !allowed {
                    self.emit_diagnostic(Diagnostic::property_is_protected(prop_span, prop_name));
                }
            }
        }
    }

    pub(in crate::check::checker) fn has_exact_class_access_context(&self, owner: ClassId) -> bool {
        self.current_class == Some(owner) || self.enclosing_classes.contains(&owner)
    }

    pub(in crate::check::checker) fn has_derived_class_access_context(
        &self,
        owner: ClassId,
    ) -> bool {
        self.current_class
            .into_iter()
            .chain(self.enclosing_classes.iter().copied())
            .any(|context| self.is_class_or_subclass(context, owner))
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
        let source = match self.demand_composite_apparent_type(source) {
            DemandOutcome::Ready(source) => source,
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(
                    DemandOutcome::Exhausted(exhaustion),
                    Span::from_oxc(pattern.span),
                );
                return;
            }
        };
        for property in &pattern.properties {
            // The checked name is the pattern KEY (`{ priv: a }` checks `priv`). A
            // computed / non-static key yields `None` and is skipped.
            let Some(name) = property.key.static_name() else {
                continue;
            };
            // Resolve the member's visibility + origin on the source type, then run the
            // shared access check. `None` = not found → stay silent (no TK2339 here).
            let span = Span::from_oxc(property.key.span());
            match self.pattern_member_access(source, &name) {
                DemandOutcome::Ready(Some((visibility, declaring_class))) => {
                    self.check_member_access_control(&name, span, visibility, declaring_class);
                }
                DemandOutcome::Ready(None) => {}
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), span);
                    return;
                }
            }
        }
    }

    /// Look up a destructured member's access-control metadata.
    /// Objects are direct; unions require the member on every constituent. Missing
    /// means "no check" per the destructuring `TK2339` deferral, and `any`/error
    /// constituents contribute no nominal origin.
    fn pattern_member_access(
        &mut self,
        source: TypeId,
        name: &str,
    ) -> DemandOutcome<Option<(Visibility, Option<ClassId>)>> {
        self.with_semantic_query_transaction(|pass| pass.pattern_member_access_inner(source, name))
    }

    fn pattern_member_access_inner(
        &mut self,
        source: TypeId,
        name: &str,
    ) -> DemandOutcome<Option<(Visibility, Option<ClassId>)>> {
        // M24 (audit): a constrained-type-parameter source (`function f<T extends K>({
        // priv }: T)`) resolves through its apparent type, like every other structural
        // consumer. Identity for a non-parameter source.
        let source = self.apparent_type(source);
        // M31: an intersection property is present if any object member declares it;
        // keep private/protected origins so access remains gated.
        if let Some(members) = self.interner.store().intersection_members(source) {
            let members = members.to_vec();
            let mut result: Option<(Visibility, Option<ClassId>)> = None;
            for member in members {
                let member = match self.demand_apparent_type(member) {
                    DemandOutcome::Ready(member) => member,
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                };
                if let Some(prop) = self
                    .interner
                    .store()
                    .object_type(member)
                    .and_then(|o| o.property(name))
                {
                    if result.is_none()
                        || matches!(prop.visibility, Visibility::Private | Visibility::Protected)
                    {
                        result = Some((prop.visibility, prop.declaring_class));
                    }
                }
            }
            return DemandOutcome::Ready(result);
        }
        if let Some(members) = self.interner.store().union_members(source) {
            let members = members.to_vec();
            // Require the property on every constituent; report the first non-public
            // origin found so the union still gates a private/protected member.
            let wk = self.interner.well_known();
            let mut result: Option<(Visibility, Option<ClassId>)> = None;
            for member in members {
                if member == wk.any || member == wk.error {
                    continue;
                }
                // A constrained-param constituent resolves through its apparent type too.
                let member = match self.demand_apparent_type(member) {
                    DemandOutcome::Ready(member) => member,
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                };
                match self
                    .interner
                    .store()
                    .object_type(member)
                    .and_then(|o| o.property(name))
                {
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
                    None => return DemandOutcome::Ready(None),
                }
            }
            return DemandOutcome::Ready(result);
        }
        DemandOutcome::Ready(
            self.interner
                .store()
                .object_type(source)
                .and_then(|obj| obj.property(name))
                .map(|prop| (prop.visibility, prop.declaring_class)),
        )
    }
}

fn body_base_is_this(expression: &oxc_ast::ast::Expression<'_>) -> bool {
    match expression {
        oxc_ast::ast::Expression::ThisExpression(_) => true,
        oxc_ast::ast::Expression::ParenthesizedExpression(parenthesized) => {
            body_base_is_this(&parenthesized.expression)
        }
        _ => false,
    }
}
