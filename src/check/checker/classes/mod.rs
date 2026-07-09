//! classes module (extracted from checker/mod.rs).

mod inheritance;
mod members;
mod visibility;

use crate::binder::scope::ScopeId;
use crate::types::repr::{
    ClassId, FunctionType, ObjectType, ParameterType, PropertyType,
    TypeParamId, Visibility,
};
use crate::types::store::TypeId;
use crate::span::Span;
use crate::diagnostics::Diagnostic;
use crate::relate::Relater;
use oxc_ast::ast::{
    Class, ClassElement, Expression, Function,
    MethodDefinition, MethodDefinitionKind, MethodDefinitionType,
    PropertyDefinitionType, TSTypeParameterDeclaration,
};
use rustc_hash::{FxHashMap, FxHashSet};
use super::context::*;
use super::calls::{parameter_name, widen};
use super::decls::type_decl_id;
use super::decls::value_decl_id;
use super::statements::overload_implementation_compatible;
use self::visibility::{constructor_visibility, has_public_constructor, lower_visibility};

/// One accessor pair being assembled into a single property.
///
/// Getter type wins, getter-only is `readonly`, and getter accessibility wins when a
/// pair is present.
struct AccessorBuild {
    name: String,
    /// The getter's return type, if a `get` accessor of this name was seen and its
    /// return annotation lowered. `None` for a set-only (deferred) or unlowerable
    /// getter. This is the accessor property's type.
    getter_ret: Option<TypeId>,
    /// Whether a `set` accessor of this name was seen. Drives `readonly`: a getter with
    /// **no** setter is a read-only property. (The setter's parameter *type* is not
    /// retained — with the getter's type taken as the property type and set-only
    /// deferred, only the setter's *presence* matters here.)
    has_setter: bool,
    /// The accessor's access modifier (getter's preferred when both present).
    visibility: Visibility,
}

struct ClassOwnMembers {
    instance: Vec<PropertyType>,
    static_side: Vec<PropertyType>,
    ctor_params: Option<Vec<ParameterType>>,
    ctor_overloads: Option<Vec<TypeId>>,
}

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Fill a class on demand, base first, without recursing through `extends` cycles.
    pub(in crate::check::checker) fn ensure_class_filled(&mut self, scope: ScopeId, index: usize) {
        match self.class_fill.get(index).copied() {
            // Already filled, or filling (an `extends` cycle re-entered this class) — do
            // not recurse. The in-progress class composes against whatever its base has
            // contributed so far (nothing, on a direct cycle), which terminates lowering.
            Some(ClassFillState::Done) | Some(ClassFillState::Filling) => return,
            Some(ClassFillState::Pending) => {}
            // Out of range / not a class: nothing to fill.
            None => return,
        }
        let TypeDecl::Class {
            reserved,
            class_id,
            ref params,
            param_decl,
            class,
        } = self.type_decls[index]
        else {
            return;
        };
        // M16: snapshot the class's type-parameter ids (the borrow on `type_decls` must not
        // overlap the `&mut pass` `fill_class` call below).
        let params = params.clone();

        // Mark in-progress before resolving the base, so a cyclic `extends` is caught by
        // the `Filling` guard above rather than looping.
        if let Some(slot) = self.class_fill.get_mut(index) {
            *slot = ClassFillState::Filling;
        }

        self.fill_class(scope, reserved, class_id, &params, param_decl, class);

        if let Some(slot) = self.class_fill.get_mut(index) {
            *slot = ClassFillState::Done;
        }
    }

    /// Resolve and fill a plain-identifier base class; out-of-subset or cyclic bases
    /// contribute no base class info.
    fn resolve_base_class(&mut self, scope: ScopeId, class: &Class<'_>) -> Option<ClassInfo> {
        let Expression::Identifier(ident) = class.super_class.as_ref()? else {
            return None;
        };
        let base_name = ident.name.as_str();

        // Fill the base first so its instance type + constructor exist before compose.
        // Type `DeclId`s and `type_decls` are both source-ordered, so the id index is
        // the `type_decls` index.
        if let Some(type_id) = type_decl_id(self.binder, scope, base_name) {
            self.ensure_class_filled(scope, type_id.index());
        }

        // Read the base's `ClassInfo` via its *value* slot (where classes register their
        // `new`-info). Absent when the name is not a class or the base is mid-fill (cycle).
        let decl_id = value_decl_id(self.binder, scope, base_name)?;
        self.class_ctors.get(&decl_id).copied()
    }

    /// Build the class instance type, static side, constructor signature, and metadata.
    /// This is type-only lowering; bodies are checked later after `this`, `super`,
    /// and class type parameters are bound.
    fn fill_class(
        &mut self,
        scope: ScopeId,
        reserved: TypeId,
        class_id: ClassId,
        type_params: &[TypeParamId],
        param_decl: Option<&TSTypeParameterDeclaration<'_>>,
        class: &Class<'_>,
    ) {
        let void_ty = self.interner.well_known().void;

        // Fill the base before composition; unresolved or cyclic bases contribute nothing.
        let base = self.resolve_base_class(scope, class);

        // `protected` access walks this stable parent chain, not object identities.
        if let Some(base_info) = base {
            self.class_parents.insert(class_id, base_info.class_id);
        }

        // Compose pending abstract members now so subclasses inherit the same list.
        let base_decl_id = base_class_name(class)
            .and_then(|name| value_decl_id(self.binder, scope, name));
        let (own_abstract, own_concrete) = collect_abstract_members(class);
        let base_pending = base_decl_id
            .and_then(|decl_id| self.class_pending_abstract.get(&decl_id).cloned())
            .unwrap_or_default();
        let mut pending: Vec<String> = own_abstract.clone();
        for member in base_pending {
            // An own concrete member implements it; an own abstract redeclaration does
            // not (it is already carried via `own_abstract`, so skip it to avoid a dup).
            if !own_concrete.contains(&member) && !own_abstract.contains(&member) {
                pending.push(member);
            }
        }
        if !class.r#abstract {
            self.report_missing_abstract_members(class, &pending);
        }

        // Build before the closure because interning the frame needs `&mut self`.
        let frame = self.build_type_param_frame(param_decl, type_params);

        // Lower own members inside the class type-parameter frame.
        let own_members = self.with_type_params(frame, |pass| {
            // M24: lower the parameters' `extends` constraints with the frame active.
            pass.lower_type_param_constraints(scope, param_decl, type_params);
            pass.collect_class_own_members(scope, class_id, class)
        });

        // Snapshot base members so the immutable store borrow ends before interning.
        let base_instance: Vec<PropertyType> = base
            .and_then(|info| self.interner.store().object_type(info.instance))
            .map(|obj| obj.properties.clone())
            .unwrap_or_default();
        let base_static: Vec<PropertyType> = base
            .and_then(|info| self.interner.store().object_type(info.static_side))
            .map(|obj| obj.properties.clone())
            .unwrap_or_default();

        // Keep declaration kinds for subclasses; TK2416 variance keys on the base kind.
        let base_member_kinds = base_decl_id
            .and_then(|decl_id| self.class_member_kinds.get(&decl_id).cloned())
            .unwrap_or_default();
        let mut member_kinds = base_member_kinds.clone();
        member_kinds.extend(own_instance_member_kinds(class));

        // Public override checks run later; generic bases are deferred to avoid false positives.
        let base_is_generic = base_decl_id
            .map(|decl_id| self.class_type_params.contains_key(&decl_id))
            .unwrap_or(false);
        if type_params.is_empty() && !base_is_generic {
            self.collect_override_checks(
                class,
                &own_members.instance,
                &base_instance,
                &base_member_kinds,
            );
        }

        let properties = compose_members(base_instance, own_members.instance);

        let static_properties = compose_members(base_static, own_members.static_side);

        // Fill the reserved instance type with the composed (base + own) members.
        self.interner.fill_object(
            reserved,
            ObjectType {
                properties,
                ..Default::default()
            },
        );

        // A class with no own constructor inherits the base constructor's accessibility.
        let (ctor_visibility, ctor_declaring_class) = if own_members.ctor_params.is_some() {
            (constructor_visibility(class), class_id)
        } else {
            match base {
                Some(base_info) => (base_info.ctor_visibility, base_info.ctor_declaring_class),
                None => (Visibility::Public, class_id),
            }
        };

        // `new` reads only the parameters; the function return is unused.
        let ctor = match own_members.ctor_params {
            Some(params) => self.interner.intern_function(FunctionType {
                params,
                ret: void_ty,
            }),
            None => match base {
                Some(base_info) => base_info.ctor,
                None => self.interner.intern_function(FunctionType {
                    params: Vec::new(),
                    ret: void_ty,
                }),
            },
        };

        // F1/WU3: expose a public construct signature on the class value for
        // relation only. Direct `new Class(...)` still uses `class_ctors`; abstract
        // classes and private/protected constructors expose no public construct side.
        let construct_signatures = if class.r#abstract || !has_public_constructor(class) {
            Vec::new()
        } else if let Some(overloads) = &own_members.ctor_overloads {
            let overload_params: Vec<Vec<ParameterType>> = overloads
                .iter()
                .filter_map(|ctor| {
                    self.interner
                        .store()
                        .function_type(*ctor)
                        .map(|func| func.params.clone())
                })
                .collect();
            overload_params
                .into_iter()
                .map(|params| self.interner.intern_function(FunctionType { params, ret: reserved }))
                .collect()
        } else {
            let params = self
                .interner
                .store()
                .function_type(ctor)
                .map(|func| func.params.clone())
                .unwrap_or_default();
            vec![self.interner.intern_function(FunctionType {
                params,
                ret: reserved,
            })]
        };

        // M13: build the static-side object type (the class value's type). Composed from
        // the base's static side and the class's own static members.
        let static_side = self.interner.intern_object(ObjectType {
            properties: static_properties,
            construct_signatures,
            // M19: a class's instance/static side carries no index signature in this subset.
            ..Default::default()
        });

        // The base's constructor signature (M12), for checking `super(args)` in this
        // class's constructor body. `None` for a class with no resolvable base.
        let super_ctor = base.map(|base_info| base_info.ctor);

        // Register the class's `new`-info under its VALUE-space `DeclId` (the constructor
        // side), so `new ClassName(args)` resolves it via the value slot.
        if let Some(id) = &class.id {
            // Backlog 20: record the class's display name, keyed by `ClassId`, so
            // `infer_new` can name the constructor's **declaring** class (possibly a
            // base) in a `TK2673`/`TK2674` message.
            self.class_names.insert(class_id, id.name.to_string());
            if let Some(decl_id) = value_decl_id(self.binder, scope, id.name.as_str()) {
                self.class_ctors.insert(
                    decl_id,
                    ClassInfo {
                        ctor,
                        instance: reserved,
                        static_side,
                        class_id,
                        super_ctor,
                        // M15: only this class's own `abstract` keyword matters for `new`.
                        is_abstract: class.r#abstract,
                        // Backlog 20: the constructor's visibility + declaring class
                        // (inherited from the base when this class has no own ctor).
                        ctor_visibility,
                        ctor_declaring_class,
                    },
                );
                // M16: map the class value `DeclId` to its type parameters so `infer_new`
                // can substitute the constructor + instance template. Kept outside
                // `ClassInfo` so that stays `Copy`.
                if !type_params.is_empty() {
                    self.class_type_params.insert(decl_id, type_params.to_vec());
                }
                if let Some(overloads) = own_members.ctor_overloads.clone() {
                    self.class_ctor_overloads.insert(decl_id, overloads);
                }
                // Backlog 06: record this class's pending abstract members so a
                // subclass composes against it (absent = empty). Non-abstract classes
                // that keep pending members already reported above; still stored so a
                // further subclass sees the same unimplemented set.
                if !pending.is_empty() {
                    self.class_pending_abstract.insert(decl_id, pending.clone());
                }
                // Backlog 06: record the composed member-kind map so a subclass keys
                // its TK2416 variance split on this (its base) chain's kinds.
                if !member_kinds.is_empty() {
                    self.class_member_kinds.insert(decl_id, member_kinds);
                }
                // M13: the class value resolves to its static-side object type;
                // direct construction still uses `class_ctors`. Instance members are
                // absent from this side, so `C.instanceMember` is `TK2339`.
                self.decl_types.set(decl_id, static_side);
            }
        }
    }

    /// Collect a class's own instance/static members and constructor parameters.
    /// Runs inside the class type-parameter frame, stamps visibility + declaring
    /// [`ClassId`], and leaves base composition/interning to the caller.
    /// Unsupported members are skipped; the class keeps what it can express.
    fn collect_class_own_members(
        &mut self,
        scope: ScopeId,
        class_id: ClassId,
        class: &Class<'_>,
    ) -> ClassOwnMembers {
        let mut own_instance: Vec<PropertyType> = Vec::new();
        let mut own_static: Vec<PropertyType> = Vec::new();
        // The constructor's parameters, if an explicit `constructor` is present.
        let mut ctor_params: Option<Vec<ParameterType>> = None;
        let mut ctor_overloads: Vec<(TypeId, Span)> = Vec::new();
        let overloaded_methods = class_overloaded_method_names(class);
        let mut lowered_overloaded_methods: FxHashSet<(String, bool)> = FxHashSet::default();
        // M15: accumulate get/set pairs per name, then build one accessor property.
        // A getter supplies the type; a missing setter makes it `readonly`.
        let mut accessors: Vec<AccessorBuild> = Vec::new();

        for element in &class.body.body {
            match element {
                // A field becomes a static or instance property. Annotated fields use
                // the declared type; unannotated initialized fields infer it. Computed
                // or untyped/uninitialized fields are skipped.
                ClassElement::PropertyDefinition(prop) => {
                    if prop.computed {
                        continue;
                    }
                    let Some(name) = prop.key.static_name() else {
                        continue;
                    };
                    let Some(ty) = (match prop.type_annotation.as_ref() {
                        // Annotated: lower the declared type (M11). `None` (unlowerable /
                        // out of subset) keeps the field skipped.
                        Some(annotation) => self.lower_annotation(scope, &annotation.type_annotation),
                        // Type-only inference: phase 2 re-walks the initializer and is the
                        // sole emitter, so snapshot+restore diagnostics and obligations here
                        // to avoid double reports. Non-`readonly` literals widen; `readonly`
                        // keeps the literal.

                        // For `this` in an initializer, bind only members collected so far.
                        // Backward references infer real types; self/forward references are
                        // absent, resolve to error, and cannot recurse. Method calls read
                        // lowered signatures, never another field initializer.
                        None => prop.value.as_ref().and_then(|init| {
                            let members_so_far = if prop.r#static {
                                own_static.clone()
                            } else {
                                own_instance.clone()
                            };
                            let this_ty = self.interner.intern_object(ObjectType {
                                properties: members_so_far,
                                ..Default::default()
                            });
                            let saved_this = self.current_this;
                            // Same `current_class` as the body walk would use, so access
                            // control over `this.private` behaves identically (its diagnostics
                            // are truncated here regardless).
                            let saved_class = self.current_class;
                            self.current_this = Some(this_ty);
                            self.current_class = Some(class_id);
                            let saved_diags = self.diagnostics.len();
                            let saved_obls = self.obligations.len();
                            let inferred = self.infer_expr(scope, init).map(|(ty, _)| ty);
                            self.diagnostics.truncate(saved_diags);
                            self.obligations.truncate(saved_obls);
                            self.current_this = saved_this;
                            self.current_class = saved_class;
                            inferred.map(|ty| {
                                if prop.readonly {
                                    ty
                                } else {
                                    widen(self.interner, ty)
                                }
                            })
                        }),
                    }) else {
                        continue;
                    };
                    // M21: optional fields are real members with `| undefined` baked in
                    // here, where interning is available; the relation engine cannot intern.
                    let ty = if prop.optional {
                        self.optional_field_effective_type(ty)
                    } else {
                        ty
                    };
                    let member = PropertyType {
                        name: name.into_owned(),
                        ty,
                        optional: prop.optional,
                        visibility: lower_visibility(prop.accessibility),
                        declaring_class: Some(class_id),
                        // M14: carry the `readonly` modifier into the member's identity.
                        // It does not affect assignability (the relation ignores it); it
                        // only gates assignment targets (`TK2540`).
                        readonly: prop.readonly,
                        // M15: a data field is not an accessor.
                        is_accessor: false,
                    };
                    if prop.r#static {
                        own_static.push(member);
                    } else {
                        own_instance.push(member);
                    }
                }
                // Constructors record parameter signatures; methods become function-typed
                // properties; accessors are accumulated and combined after the loop.
                ClassElement::MethodDefinition(method) => {
                    if method.computed {
                        continue;
                    }
                    match method.kind {
                        MethodDefinitionKind::Constructor => {
                            if method.value.body.is_none() {
                                let params =
                                    self.lower_signature_parameters(scope, &method.value.params);
                                let signature = self.interner.intern_function(FunctionType {
                                    params,
                                    ret: self.interner.well_known().void,
                                });
                                ctor_overloads.push((signature, Span::from_oxc(method.span)));
                                continue;
                            }
                            // The constructor signature: its parameters (used by `new`).
                            // A field/sibling-class reference in a parameter annotation
                            // resolves from `scope`. A `static` keyword on a constructor
                            // is not valid TS; treat any constructor as the constructor.
                            ctor_params =
                                Some(self.lower_signature_parameters(scope, &method.value.params));
                            // A constructor parameter property also declares an instance
                            // member with the param's modifier and annotated type; an
                            // unmodified param stays only a parameter.
                            for param in &method.value.params.items {
                                if param.accessibility.is_none() && !param.readonly {
                                    continue;
                                }
                                let Some(name) = parameter_name(&param.pattern) else {
                                    continue;
                                };
                                let error_ty = self.interner.well_known().error;
                                let ty = match param.type_annotation.as_ref() {
                                    Some(ann) => self
                                        .lower_annotation(scope, &ann.type_annotation)
                                        .unwrap_or(error_ty),
                                    None => error_ty,
                                };
                                own_instance.push(PropertyType {
                                    name,
                                    ty,
                                    optional: false,
                                    // A private/protected parameter property makes the class
                                    // nominal, exactly like an annotated private/protected
                                    // field (same `visibility` + `declaring_class` identity).
                                    visibility: lower_visibility(param.accessibility),
                                    declaring_class: Some(class_id),
                                    // M14: a `readonly` parameter property gates assignment
                                    // (`TK2540`) but not assignability (the relation ignores it).
                                    readonly: param.readonly,
                                    is_accessor: false,
                                });
                            }
                        }
                        MethodDefinitionKind::Method => {
                            let Some(name) = method.key.static_name() else {
                                continue;
                            };
                            let overload_key = (name.to_string(), method.r#static);
                            if overloaded_methods.contains(&overload_key) {
                                if lowered_overloaded_methods.insert(overload_key.clone()) {
                                    if let Some(member) = self.lower_class_method_overload(
                                        scope,
                                        class_id,
                                        class,
                                        name.as_ref(),
                                        method.r#static,
                                        lower_visibility(method.accessibility),
                                    ) {
                                        if method.r#static {
                                            own_static.push(member);
                                        } else {
                                            own_instance.push(member);
                                        }
                                    }
                                }
                                continue;
                            }
                            let Some(ty) = self.lower_method_signature(scope, &method.value) else {
                                continue;
                            };
                            let member = PropertyType {
                                name: name.into_owned(),
                                ty,
                                optional: false,
                                visibility: lower_visibility(method.accessibility),
                                declaring_class: Some(class_id),
                                // A method is never `readonly` (the modifier is a no-op
                                // on methods); only data members carry it.
                                readonly: false,
                                // M15: a plain (or abstract) method is not an accessor.
                                is_accessor: false,
                            };
                            if method.r#static {
                                own_static.push(member);
                            } else {
                                own_instance.push(member);
                            }
                        }
                        // M15: instance accessors record getter/setter data per name and
                        // build one property after the loop. Static accessors are deferred,
                        // though their bodies are still walked by `check_class`.
                        MethodDefinitionKind::Get | MethodDefinitionKind::Set => {
                            if method.r#static {
                                continue;
                            }
                            let Some(name) = method.key.static_name() else {
                                continue;
                            };
                            self.record_accessor(scope, &mut accessors, name.as_ref(), method);
                        }
                    }
                }
                // Static blocks, accessor properties, index signatures: out of subset.
                _ => {}
            }
        }

        // M15: turn each accumulated getter/setter into ONE accessor property (type from
        // the getter's return / setter's parameter; `readonly` for a get-only accessor) and
        // add it to the class's own **instance** members. Accessors are always instance-side
        // here (static accessors are deferred, skipped above).
        for member in build_accessor_members(class_id, accessors) {
            own_instance.push(member);
        }

        if let Some(params) = &ctor_params {
            let implementation = self.interner.intern_function(FunctionType {
                params: params.clone(),
                ret: self.interner.well_known().void,
            });
            self.check_class_overload_compatibility(implementation, &ctor_overloads);
        }

        let ctor_overloads = if ctor_overloads.is_empty() {
            None
        } else {
            Some(ctor_overloads.iter().map(|(signature, _)| *signature).collect())
        };
        ClassOwnMembers {
            instance: own_instance,
            static_side: own_static,
            ctor_params,
            ctor_overloads,
        }
    }

    fn lower_class_method_overload(
        &mut self,
        scope: ScopeId,
        class_id: ClassId,
        class: &Class<'_>,
        name: &str,
        is_static: bool,
        visibility: Visibility,
    ) -> Option<PropertyType> {
        let mut call_signatures: Vec<TypeId> = Vec::new();
        let mut overloads: Vec<(TypeId, Span)> = Vec::new();
        let mut implementation: Option<TypeId> = None;
        let mut unsupported = false;
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.computed
                || method.r#static != is_static
                || method.kind != MethodDefinitionKind::Method
                || method.key.static_name().as_deref() != Some(name)
            {
                continue;
            }
            if method.value.type_parameters.is_some() {
                unsupported = true;
                continue;
            }
            let signature = self.lower_method_signature(scope, &method.value)?;
            if method.value.body.is_some() {
                implementation = Some(signature);
                continue;
            }
            call_signatures.push(signature);
            overloads.push((signature, Span::from_oxc(method.span)));
        }
        if unsupported {
            return Some(PropertyType {
                name: name.to_string(),
                ty: self.interner.well_known().never,
                optional: false,
                visibility,
                declaring_class: Some(class_id),
                readonly: false,
                is_accessor: false,
            });
        }
        if call_signatures.is_empty() {
            return None;
        }
        if let Some(implementation) = implementation {
            self.check_class_overload_compatibility(implementation, &overloads);
        }
        let ty = self.interner.intern_object(ObjectType {
            call_signatures,
            ..Default::default()
        });
        Some(PropertyType {
            name: name.to_string(),
            ty,
            optional: false,
            visibility,
            declaring_class: Some(class_id),
            readonly: false,
            is_accessor: false,
        })
    }

    fn check_class_overload_compatibility(
        &mut self,
        implementation_ty: TypeId,
        overloads: &[(TypeId, Span)],
    ) {
        for (signature_ty, span) in overloads {
            let wk = self.interner.well_known();
            let store = self.interner.store();
            let mut relater = Relater::new(store, wk);
            if !overload_implementation_compatible(
                store,
                &mut relater,
                *signature_ty,
                implementation_ty,
            ) {
                self.diagnostics
                    .push(Diagnostic::overload_incompatible(*span));
                break;
            }
        }
    }

    /// Record one getter/setter into the per-name accessor accumulator.
    /// The getter supplies the property type; the setter only marks writability.
    /// Matching get/set declarations merge, and unlowerable getter types build no
    /// property. Differing get/set types remain deferred.
    fn record_accessor(
        &mut self,
        scope: ScopeId,
        accessors: &mut Vec<AccessorBuild>,
        name: &str,
        method: &MethodDefinition<'_>,
    ) {
        // A getter's return type is the accessor property's type, lowered from `scope`
        // (where field/sibling-class type names live). A setter contributes no type here —
        // only its presence (which makes the property writable / not `readonly`).
        let getter_ret = match method.kind {
            MethodDefinitionKind::Get => method
                .value
                .return_type
                .as_ref()
                .and_then(|ann| self.lower_annotation(scope, &ann.type_annotation)),
            _ => None,
        };
        let visibility = lower_visibility(method.accessibility);

        // The index of this name's entry (the matching get/set), creating a fresh entry
        // when none exists yet. Resolving an index — rather than holding a `&mut` from
        // `find` — keeps the "create then update" path borrow-clean (no `unwrap`/`expect`).
        let index = match accessors.iter().position(|a| a.name == name) {
            Some(index) => index,
            None => {
                accessors.push(AccessorBuild {
                    name: name.to_string(),
                    getter_ret: None,
                    has_setter: false,
                    visibility,
                });
                accessors.len() - 1
            }
        };

        // Update the entry in place. The index is in range (just found or just pushed); a
        // defensive `get_mut` still avoids any panic.
        let Some(entry) = accessors.get_mut(index) else {
            return;
        };
        match method.kind {
            MethodDefinitionKind::Get => {
                entry.getter_ret = getter_ret;
                // A getter's accessibility takes precedence when both are present.
                entry.visibility = visibility;
            }
            MethodDefinitionKind::Set => entry.has_setter = true,
            MethodDefinitionKind::Method | MethodDefinitionKind::Constructor => {}
        }
    }

    /// Lower a method signature to the function type stored as its property.
    /// Parameters are positional annotations; an omitted return is `void`.
    /// Returns `None` only when a present return annotation cannot be lowered.
    fn lower_method_signature(&mut self, scope: ScopeId, func: &Function<'_>) -> Option<TypeId> {
        self.lower_signature_function_type(scope, &func.params, func.return_type.as_deref())
    }

    /// Effective optional-field type shared by member construction and initializer checks.
    fn optional_field_effective_type(&mut self, ty: TypeId) -> TypeId {
        let undefined = self.interner.well_known().undefined;
        self.interner.union(vec![ty, undefined])
    }
}

/// Build one class property per accumulated accessor name.
/// A getter supplies the property type; a get-only accessor is `readonly`, while
/// get+set is writable. Set-only and unlowerable accessors are deferred/skipped,
/// and ordinary member read/assignment machinery handles the resulting property.
fn build_accessor_members(class_id: ClassId, accessors: Vec<AccessorBuild>) -> Vec<PropertyType> {
    let mut members: Vec<PropertyType> = Vec::with_capacity(accessors.len());
    for accessor in accessors {
        // The property type is the getter's return type. A get-only or get+set accessor
        // has one; a set-only accessor (no getter) is deferred → build nothing. An
        // unlowerable getter return type also yields no property (skip rather than guess).
        let ty = match accessor.getter_ret {
            Some(ty) => ty,
            None => continue,
        };
        // A getter exists; it is `readonly` exactly when there is no setter (get-only).
        let readonly = !accessor.has_setter;
        members.push(PropertyType {
            name: accessor.name,
            ty,
            optional: false,
            visibility: accessor.visibility,
            declaring_class: Some(class_id),
            readonly,
            // M15: mark this as an accessor so a get-only accessor (`readonly: true`) is
            // distinguished from a `readonly` data field — the constructor carve-out in
            // `check_member_assignment` applies to fields only, so a get-only accessor is
            // `TK2540` even inside its declaring constructor (matching tsc).
            is_accessor: true,
        });
    }
    members
}

/// The direct base class's **name** from a plain-identifier `extends` clause
/// (backlog 06). `None` when the class has no `extends`, or the clause is not a
/// plain identifier (`extends mixin(Base)` / `extends Box<string>` with a
/// non-identifier callee) — the same subset [`resolve_base_class`] recognizes.
fn base_class_name<'x, 'ast>(class: &'x Class<'ast>) -> Option<&'x str> {
    match class.super_class.as_ref()? {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

/// Declaration kind for each own instance member: `true` for method syntax,
/// `false` for fields/accessors/parameter properties. Last duplicate wins, and
/// `fill_class` overlays this onto the base chain for the `TK2416` base-kind rule.
fn own_instance_member_kinds(class: &Class<'_>) -> FxHashMap<String, bool> {
    let mut kinds: FxHashMap<String, bool> = FxHashMap::default();
    for element in &class.body.body {
        match element {
            ClassElement::PropertyDefinition(prop) => {
                if prop.computed || prop.r#static {
                    continue;
                }
                if let Some(name) = prop.key.static_name() {
                    kinds.insert(name.into_owned(), false);
                }
            }
            ClassElement::MethodDefinition(method) => {
                if method.computed || method.r#static {
                    continue;
                }
                match method.kind {
                    // A parameter property declares a (non-method) instance field.
                    MethodDefinitionKind::Constructor => {
                        for param in &method.value.params.items {
                            if param.accessibility.is_none() && !param.readonly {
                                continue;
                            }
                            if let Some(name) = parameter_name(&param.pattern) {
                                kinds.insert(name, false);
                            }
                        }
                    }
                    MethodDefinitionKind::Method => {
                        if let Some(name) = method.key.static_name() {
                            kinds.insert(name.into_owned(), true);
                        }
                    }
                    // An accessor is not a method for the variance rule.
                    MethodDefinitionKind::Get | MethodDefinitionKind::Set => {
                        if let Some(name) = method.key.static_name() {
                            kinds.insert(name.into_owned(), false);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    kinds
}

fn class_overloaded_method_names(class: &Class<'_>) -> FxHashSet<(String, bool)> {
    let mut counts: FxHashMap<(String, bool), usize> = FxHashMap::default();
    for element in &class.body.body {
        let ClassElement::MethodDefinition(method) = element else {
            continue;
        };
        if method.computed || method.kind != MethodDefinitionKind::Method {
            continue;
        }
        if let Some(name) = method.key.static_name() {
            *counts.entry((name.into_owned(), method.r#static)).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter_map(|(key, count)| if count > 1 { Some(key) } else { None })
        .collect()
}

/// Own abstract member names and own concrete implementations for abstract-completeness.
/// Concrete members include non-abstract methods, fields, accessors, and parameter
/// properties. Static/computed members are ignored when composing pending names down
/// the `extends` chain.
fn collect_abstract_members(class: &Class<'_>) -> (Vec<String>, FxHashSet<String>) {
    let mut own_abstract: Vec<String> = Vec::new();
    let mut own_concrete: FxHashSet<String> = FxHashSet::default();
    let mut push_abstract = |name: String| {
        if !own_abstract.contains(&name) {
            own_abstract.push(name);
        }
    };
    for element in &class.body.body {
        match element {
            ClassElement::PropertyDefinition(prop) => {
                if prop.computed || prop.r#static {
                    continue;
                }
                let Some(name) = prop.key.static_name() else {
                    continue;
                };
                if prop.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition {
                    push_abstract(name.into_owned());
                } else {
                    own_concrete.insert(name.into_owned());
                }
            }
            ClassElement::MethodDefinition(method) => {
                if method.computed || method.r#static {
                    continue;
                }
                match method.kind {
                    // Parameter properties are concrete instance fields.
                    MethodDefinitionKind::Constructor => {
                        for param in &method.value.params.items {
                            if param.accessibility.is_none() && !param.readonly {
                                continue;
                            }
                            if let Some(name) = parameter_name(&param.pattern) {
                                own_concrete.insert(name);
                            }
                        }
                    }
                    MethodDefinitionKind::Method
                    | MethodDefinitionKind::Get
                    | MethodDefinitionKind::Set => {
                        let Some(name) = method.key.static_name() else {
                            continue;
                        };
                        if method.r#type == MethodDefinitionType::TSAbstractMethodDefinition {
                            push_abstract(name.into_owned());
                        } else {
                            own_concrete.insert(name.into_owned());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (own_abstract, own_concrete)
}

/// Compose a derived instance from base members plus own overrides.
/// Own members replace same-named base members, then the interner canonicalizes
/// ordering; override compatibility is checked separately before this replacement.
fn compose_members(
    base_properties: Vec<PropertyType>,
    own_properties: Vec<PropertyType>,
) -> Vec<PropertyType> {
    let mut composed: Vec<PropertyType> = base_properties;
    for own in own_properties {
        match composed.iter_mut().find(|p| p.name == own.name) {
            // Own member overrides the base member of the same name (derived wins).
            Some(existing) => *existing = own,
            // A new member: append it.
            None => composed.push(own),
        }
    }
    composed
}
