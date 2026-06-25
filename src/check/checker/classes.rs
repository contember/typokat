//! classes module (extracted from checker/mod.rs).

use crate::binder::scope::ScopeId;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{
    ClassId, FunctionType, ObjectType, ParameterType, PropertyType,
    TypeParamId, Visibility,
};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    Class, ClassElement, Expression, FormalParameters,
    Function, MethodDefinition, MethodDefinitionKind, TSAccessibility, TSTypeParameterDeclaration,
};
use super::context::*;
use super::calls::{parameter_name, widen};
use super::decls::type_decl_id;
use super::decls::value_decl_id;

/// One accessor (`get`/`set`) being assembled in [`fill_class`] (M15). A getter and a
/// same-named setter are combined into **one** property: the getter contributes the
/// property's type (its return type), the setter contributes it (its parameter type),
/// and a getter **with no setter** is `readonly` (a get-only accessor behaves like a
/// read-only property → assigning it is `TK2540`, reusing the M14 `readonly` flag).
///
/// The accessibility is taken from whichever accessor is seen (a getter's wins if both
/// are present); valid TS keeps them consistent, and the fixtures use no modifier.
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

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Fill a class's instance type + constructor (M12), filling its **base first** on
    /// demand and recording its [`ClassInfo`]. Idempotent (a `Done` class returns
    /// immediately) and **cycle-guarded** (a class re-entered while `Filling` — only
    /// reachable via an `extends` cycle — does not recurse; see [`ClassFillState`]), so
    /// it can be called in any order and any number of times. `index` is the class's
    /// position in [`Pass::type_decls`].
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

    /// Resolve a class's **base class** [`ClassInfo`] from its `extends` clause (M12),
    /// filling the base first (on demand) so its instance type / constructor are ready to
    /// compose against.
    ///
    /// The base is the `super_class` expression when it is a plain identifier resolving
    /// (through the scope graph's value slot) to a known class. Returns `None` when the
    /// class has no `extends`, the clause is not a plain identifier (out of subset — e.g.
    /// `extends mixin(Base)`), the name does not resolve to a class, or the base is
    /// re-entered mid-fill (an `extends` cycle — the base's [`ClassInfo`] is not yet
    /// registered, so composition proceeds with no base contribution, breaking the cycle
    /// without a panic).
    fn resolve_base_class(&mut self, scope: ScopeId, class: &Class<'_>) -> Option<ClassInfo> {
        let Expression::Identifier(ident) = class.super_class.as_ref()? else {
            return None;
        };
        let base_name = ident.name.as_str();

        // Fill the base first (on demand), so its instance type + constructor exist before
        // we compose. The index is the base name's type-space `DeclId` position in
        // `type_decls` (the binder numbered type `DeclId`s in source order, and
        // `reserve_type_decls` pushed `type_decls` in that same order — so the type
        // `DeclId`'s index *is* the `type_decls` index).
        if let Some(type_id) = type_decl_id(self.binder, scope, base_name) {
            self.ensure_class_filled(scope, type_id.index());
        }

        // Read the base's `ClassInfo` via its *value* slot (where classes register their
        // `new`-info). Absent when the name is not a class or the base is mid-fill (cycle).
        let decl_id = value_decl_id(self.binder, scope, base_name)?;
        self.class_ctors.get(&decl_id).copied()
    }

    /// Build a class's instance type, **static side**, and constructor signature (M11,
    /// extended for M12 inheritance and M13 modifiers/`static`), filling the reserved
    /// instance-type object in place and registering its [`ClassInfo`] under the class's
    /// value `DeclId`. **Types only** — method/constructor bodies are checked later in
    /// the walk ([`check_class`]).
    ///
    /// The instance type's members are the class's **non-static** fields (each
    /// `PropertyDefinition` with a type annotation) plus its **non-static** methods (each
    /// `MethodDefinition` of kind `method`) as **function-typed properties** — so member
    /// access (`p.x`, `p.method`) and the structural relation reuse the M2 object rules
    /// unchanged. M13 partitions out the **static** fields/methods into a separate
    /// **static-side** object type (registered as [`ClassInfo::static_side`]); the class
    /// value resolves to it, so `C.staticMember` lands on the static side while an
    /// instance member does not (cross-access → `TK2339`). The constructor (kind
    /// `Constructor`) is on neither side; its parameters become the constructor signature
    /// (zero parameters when there is no explicit `constructor`).
    ///
    /// M13 (modifiers): every member this class declares is stamped with its
    /// `accessibility` ([`Visibility`]) and this class's [`ClassId`] (`class_id`) — the
    /// *origin* the access-control (`TK2341`/`TK2445`) and nominal relation rules key on.
    /// A `private`/`protected` member therefore makes the class **nominal** (the member's
    /// visibility + origin are part of its interned identity). Inherited base members keep
    /// the base's stamp (so an inherited `protected` member is still attributed to the
    /// base), exactly as built when the base was filled.
    ///
    /// M12 (`extends`): the instance type is the **composition** of the base's members
    /// with the class's own — the base's members first, then the class's own members
    /// **overriding** the base's on a name conflict ([`compose_members`]); the static side
    /// is composed the same way from the base's static side. Because the composed object
    /// is a width-superset of the base, subclass→base assignability falls out of the
    /// existing structural relation. A derived class with **no own `constructor`** inherits
    /// the base's constructor signature (so `new Derived(...)` checks against it); a class
    /// **with** its own constructor uses that. [`super_ctor`](ClassInfo::super_ctor)
    /// records the base's constructor for `super(args)` checking in the derived
    /// constructor body, and [`class_parents`](Pass::class_parents) records the base's
    /// `ClassId` for the `protected` subclass walk.
    ///
    /// M15 (getters/setters + `abstract`): a `get`/`set` accessor becomes ONE instance
    /// property — its type is the getter's return type (else the setter's parameter type)
    /// and it is `readonly` when get-only ([`record_accessor`]/[`build_accessor_members`]),
    /// so member read/assignment reuse the M2/M14 paths unchanged. An `abstract method(): T;`
    /// (no body) is built as a function-typed property by the ordinary `Method` arm (its body
    /// is simply absent). The class's `abstract` keyword is recorded on its [`ClassInfo`]
    /// ([`ClassInfo::is_abstract`]) and enforced at `new` ([`infer_new`] → `TK2511`).
    ///
    /// M16 (generic classes): the class's type parameters (`type_params`/`param_decl`) are
    /// scoped via the M9 [`with_type_params`] frame for the **whole** member-lowering pass, so
    /// a field/method/accessor/constructor annotation referencing `T` lowers to its interned
    /// [`TypeTag::TypeParam`] type. The reserved instance id then holds a structural
    /// **template** (`{ value: T; get: () => T }`) and the constructor signature embeds `T`
    /// too. The type-parameter ids are recorded on the class's [`ClassInfo`] sibling map
    /// ([`Pass::class_type_params`]) so `new Box<number>(…)` / `new Box(…)` can substitute them
    /// into the constructor + instance ([`infer_new`]); a `Box<number>` *type reference*
    /// instantiates the template through the M9 [`instantiate_type_reference`]. A non-generic
    /// class lowers with an empty frame and is unchanged.
    ///
    /// F3 / backlog 01: constructor **parameter properties** (`constructor(private x: number)`)
    /// now declare instance members (param's modifier + annotated type) on top of remaining
    /// constructor parameters, and a field with **no annotation but an initializer**
    /// (`g = 2`, `f = () => 1`) declares a member whose type is **inferred** from the
    /// initializer (widened when non-`readonly`). See [`collect_class_own_members`].
    ///
    /// DEFERRED (out of scope, skipped without error): set-only / differing-get-set-type /
    /// `static` accessors, `implements`, method-**override compatibility** (`TK2416`), the
    /// abstract-member-not-implemented completeness check (`TK2515`), `super.method()` (super
    /// property access), generic **method-level** type parameters on a class, and **generic
    /// class inheritance** with a base's type arguments substituted (`class S extends Box<string>`
    /// composes against `Box`'s *unsubstituted* template, so a `super(...)`/inherited-member
    /// check sees the base's free `T` — no crash, terminates, but over-reports; none of the M16
    /// fixtures extend a generic base). A field with neither annotation nor initializer, a
    /// computed key, or a member whose type cannot be lowered, is skipped — each side keeps
    /// the members it can express.
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

        // M12: resolve the base class (filling it first) so its members + constructor are
        // available to compose against. `None` for a class with no `extends` (or an
        // out-of-subset / cyclic base — composition then proceeds with no base part).
        let base = self.resolve_base_class(scope, class);

        // M13: record the base's `ClassId` as this class's parent (for the `protected`
        // subclass walk). A class with no resolvable base has no parent entry.
        if let Some(base_info) = base {
            self.class_parents.insert(class_id, base_info.class_id);
        }

        // M16: the class's type-parameter frame (name → interned `TypeParam`). Built before
        // lowering so a field/method/ctor/accessor annotation referencing `T` resolves to its
        // parameter type; pushed for the whole member-lowering pass below (an empty frame for
        // a non-generic class — harmless). The frame must be built BEFORE the closure (it
        // borrows `&mut pass` to intern the parameter types), then moved into
        // `with_type_params`.
        let frame = self.build_type_param_frame(param_decl, type_params);

        // The class's **own** instance members and **own** static members, plus the
        // constructor parameters, collected **inside** the type-parameter frame so `T`
        // resolves (M16). For a non-generic class the empty frame is a no-op and the
        // collection is exactly the M11–M15 behaviour.
        let (own_instance, own_static, ctor_params) = self.with_type_params(frame, |pass| {
            pass.collect_class_own_members(scope, class_id, class)
        });

        // M12: compose the base's members with the class's own (own overriding base on a
        // name conflict), for BOTH the instance side and the static side. Snapshot the
        // base's properties into owned Vecs first — the immutable store borrow must not
        // overlap the `&mut` `fill_object`/`intern_object` below. A class with no
        // resolvable base contributes no base members (plain M11).
        let base_instance: Vec<PropertyType> = base
            .and_then(|info| self.interner.store().object_type(info.instance))
            .map(|obj| obj.properties.clone())
            .unwrap_or_default();
        let base_static: Vec<PropertyType> = base
            .and_then(|info| self.interner.store().object_type(info.static_side))
            .map(|obj| obj.properties.clone())
            .unwrap_or_default();
        let properties = compose_members(base_instance, own_instance);

        // M13: build the static-side object type (the class value's type). Composed from
        // the base's static side and the class's own static members.
        let static_properties = compose_members(base_static, own_static);
        let static_side = self.interner.intern_object(ObjectType {
            properties: static_properties,
            // M19: a class's instance/static side carries no index signature in this subset.
            ..Default::default()
        });

        // Fill the reserved instance type with the composed (base + own) members.
        self.interner.fill_object(
            reserved,
            ObjectType {
                properties,
                ..Default::default()
            },
        );

        // The constructor signature. With an explicit constructor, its parameters. With
        // none, M12: **inherit the base's constructor** (so `new Derived(...)` checks
        // against the base signature); for a class with no base either, the implicit
        // zero-parameter constructor. The return is `void` (unused — `new` yields the
        // instance type), built through the normal function interner so the call path can
        // read its parameters back out.
        let ctor = match ctor_params {
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

        // The base's constructor signature (M12), for checking `super(args)` in this
        // class's constructor body. `None` for a class with no resolvable base.
        let super_ctor = base.map(|base_info| base_info.ctor);

        // Register the class's `new`-info under its VALUE-space `DeclId` (the constructor
        // side), so `new ClassName(args)` resolves it via the value slot.
        if let Some(id) = &class.id {
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
                    },
                );
                // M16: record the class's type-parameter ids under the same value `DeclId`,
                // so `new Box<number>(…)` / `new Box(…)` ([`infer_new`]) can substitute them
                // into the (uninstantiated) constructor signature + instance template. An
                // empty list for a non-generic class (then `infer_new` skips substitution).
                // Kept in a sibling map rather than on `ClassInfo` so the latter stays `Copy`.
                if !type_params.is_empty() {
                    self.class_type_params.insert(decl_id, type_params.to_vec());
                }
                // M13: bind the class's value-slot type to the **static side**, so a
                // reference to the class *name as a value* — and the base of
                // `C.staticMember` — resolves to the static-side object type. `new` still
                // uses `class_ctors` (the constructor signature), unaffected. An instance
                // member is not on the static side, so `C.instanceMember` is `TK2339`.
                self.decl_types.set(decl_id, static_side);
            }
        }
    }

    /// Collect a class's **own** instance members, **own** static members, and constructor
    /// parameters from its body (M11–M16), stamping each member with its visibility +
    /// declaring [`ClassId`] (M13). Extracted from [`fill_class`] so the caller can run it
    /// **inside** the M16 type-parameter frame ([`with_type_params`]) — every annotation it
    /// lowers (`value: T`, `get(): T`, `constructor(v: T)`) then resolves `T` to the class's
    /// parameter. It does not compose with the base or intern anything; it only produces the
    /// class's own contribution.
    ///
    /// A field becomes a property (static-side when `static`) — typed from its annotation,
    /// or, when un-annotated with an initializer, **inferred** from the initializer (F3 —
    /// widened when non-`readonly`); a plain/`abstract` method becomes a function-typed
    /// property; a constructor records its parameter signature AND emits an instance member
    /// for each **parameter property** (a param with an accessibility / `readonly` modifier,
    /// F3); a `get`/`set` accessor is accumulated per name and combined into ONE instance
    /// property afterward ([`build_accessor_members`], M15). Computed keys, a field with
    /// neither annotation nor initializer, and out-of-subset members are skipped — the class
    /// keeps the members it can express.
    fn collect_class_own_members(
        &mut self,
        scope: ScopeId,
        class_id: ClassId,
        class: &Class<'_>,
    ) -> (Vec<PropertyType>, Vec<PropertyType>, Option<Vec<ParameterType>>) {
        let mut own_instance: Vec<PropertyType> = Vec::new();
        let mut own_static: Vec<PropertyType> = Vec::new();
        // The constructor's parameters, if an explicit `constructor` is present.
        let mut ctor_params: Option<Vec<ParameterType>> = None;
        // M15: getters/setters, accumulated per name and combined into ONE accessor
        // property after the loop. A getter contributes the property's type (its return
        // type) and a setter contributes it (its parameter type); a get without a set is
        // `readonly` (behaves like a read-only property → assigning it is `TK2540`). See
        // [`build_accessor_members`]. Keyed in declaration order so a same-named get + set
        // collapse to a single member.
        let mut accessors: Vec<AccessorBuild> = Vec::new();

        for element in &class.body.body {
            match element {
                // A field `x: T` becomes a property — on the static side when `static`,
                // otherwise on the instance. An **annotated** field takes its declared
                // type; a field with **no annotation but an initializer** (`f = () => 1`,
                // `g = 2`) takes its type **inferred from the initializer** (F3 / backlog
                // 01). Computed keys, and a field with neither annotation nor initializer,
                // are skipped (out of subset / not expressible).
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
                        // F3: no annotation — infer the type from the initializer when one
                        // is present. A field with neither annotation nor initializer stays
                        // skipped (deferred). The initializer is inferred for its TYPE ONLY:
                        // the member-body walk ([`check_class_member_bodies`]) re-walks it later
                        // (with the real `this`) and is the sole emitter, so BOTH emission
                        // channels this inference can feed — immediate `diagnostics` AND deferred
                        // `obligations` (an initializer can contain `this.x = …` assignments /
                        // return obligations) — are snapshot+restored here, else they double-
                        // report in phase 2. A non-`readonly` field **widens** its literal
                        // initializer (`g = 2` → `number`, `s = "hi"` → `string`), matching
                        // tsc; a `readonly` field keeps the literal.
                        //
                        // F3 (WU1 review fix): an initializer may reference `this`
                        // (`b = this.a`, `c = this.m()`). `fill_class` has not yet filled the
                        // reserved instance type at collection time, so we bind `this` to an
                        // object of the members collected **so far** (instance- or static-side
                        // per the field's `static`) for the duration of this type-only
                        // inference. A backward `this.x` then infers `x`'s real type instead of
                        // collapsing to `any`. This is inherently **cycle-free**: a self /
                        // forward reference (`g = this.g`, `a = this.b; b = this.a`) is not in
                        // the members-so-far, so `this.<self>` resolves to `TK2339` → the error
                        // type (truncated away) — no recursion, no hang. (Methods aren't body-
                        // inferred here; `this.m()` reads `m`'s lowered signature, so there is
                        // no recursion into another field's initializer either.)
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
                    // M21: an optional field (`name?: T`) is a real member whose effective
                    // type bakes in `| undefined` (interned here, as the relation engine
                    // cannot intern — see [`lower_interface_members`]). A class with only
                    // public fields has a structural instance type, so the optional then
                    // behaves exactly like an optional member on an object type.
                    let ty = if prop.optional {
                        let undefined = self.interner.well_known().undefined;
                        self.interner.union(vec![ty, undefined])
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
                // A method/constructor/accessor. A constructor records its parameter
                // signature; a plain `method` (including an `abstract method(): T;` with no
                // body, M15) becomes a function-typed property (static or instance); a
                // getter/setter (M15) is accumulated per name and combined into one accessor
                // property after the loop.
                ClassElement::MethodDefinition(method) => {
                    if method.computed {
                        continue;
                    }
                    match method.kind {
                        MethodDefinitionKind::Constructor => {
                            // The constructor signature: its parameters (used by `new`).
                            // A field/sibling-class reference in a parameter annotation
                            // resolves from `scope`. A `static` keyword on a constructor
                            // is not valid TS; treat any constructor as the constructor.
                            ctor_params =
                                Some(self.lower_signature_parameters(scope, &method.value.params));
                            // F3 / backlog 01: a constructor PARAMETER PROPERTY — a param
                            // carrying an accessibility (`public`/`private`/`protected`) or
                            // `readonly` modifier — ALSO declares an instance member with
                            // that modifier and the param's annotated type, IN ADDITION to
                            // remaining a constructor parameter (handled above). A param with
                            // no such modifier stays a plain parameter (no member). The
                            // member type lowers the param's annotation the same way the
                            // signature does (the error type when absent/unlowerable).
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
                        // M15: a getter/setter accessor. `static` accessors are deferred,
                        // so only instance accessors build a property (a deferred static
                        // accessor's body is still walked by `check_class`, harmlessly).
                        // The getter's return type / setter's parameter type are recorded
                        // per name and combined into one accessor property after the loop.
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

        (own_instance, own_static, ctor_params)
    }

    /// Record one getter/setter into the per-name accessor accumulator (M15). A getter
    /// records its **return type** (the accessor property's type); a setter records only
    /// that a setter exists (so a get-only accessor is `readonly`). A second accessor of the
    /// same name (the matching get/set) merges into the existing entry, so a `get`+`set` pair
    /// collapses to a single property. An unlowerable getter return type is recorded as
    /// `None` (a `None`-typed accessor builds no property); a differing get/set type is
    /// deferred (the getter's type is taken).
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

    /// Lower a method's signature to an interned **function type** for use as the
    /// instance type's property (M11). Parameters come from their annotations
    /// (positional); the return type is the declared annotation, or `void` when none is
    /// written (the property type drives member access + structural relation; the
    /// fixtures annotate method returns). Type references resolve from `scope`. Returns
    /// `None` only if the return annotation is present but cannot be lowered (out of
    /// subset) — matching the other signature lowerings.
    fn lower_method_signature(&mut self, scope: ScopeId, func: &Function<'_>) -> Option<TypeId> {
        let void_ty = self.interner.well_known().void;
        let params = self.lower_signature_parameters(scope, &func.params);
        let ret = match func.return_type.as_ref() {
            Some(ann) => self.lower_annotation(scope, &ann.type_annotation)?,
            None => void_ty,
        };
        Some(self.interner.intern_function(FunctionType { params, ret }))
    }

    /// Lower a method's/constructor's parameter list to positional `ParameterType`s for
    /// its **signature type** (M11). An un-annotated parameter is out of the MVP subset
    /// → the error type (no diagnostic), matching `lower_parameters`. Rest/optional
    /// parameters and parameter properties are out of subset; a destructured parameter
    /// has no name (defaulted empty). This builds the *signature* only; parameter symbols
    /// for the body are bound separately by `lower_parameters` during the body walk.
    fn lower_signature_parameters(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
    ) -> Vec<ParameterType> {
        let error_ty = self.interner.well_known().error;
        let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
        for param in &params.items {
            let name = parameter_name(&param.pattern).unwrap_or_default();
            let ty = match param.type_annotation.as_ref() {
                Some(ann) => self.lower_annotation(scope, &ann.type_annotation).unwrap_or(error_ty),
                None => error_ty,
            };
            lowered.push(ParameterType {
                name,
                ty,
                optional: false,
            });
        }
        lowered
    }

    /// Check a class declaration's member **bodies** (M11). The class's types (instance
    /// type + constructor signature) are already built in phase 0 ([`fill_class`]); this
    /// walks each method/constructor body so `return <expr>` is checked against the
    /// method's declared return type (`TK2322`) and any nested construct (including
    /// `this.x = …` member-assignments and binary expressions) is resolved.
    ///
    /// `this` is bound to the class's **instance type** for the duration of each member
    /// body and **restored** afterward (save/restore around the whole class), so
    /// `this.field`/`this.method()` resolve inside but `this` never leaks past the class.
    /// A method/constructor body is checked via [`infer_function`] — the same machine as
    /// a free function (parameters into the method scope, return obligations) — whose
    /// returned function *type* is discarded here (only the body-check side effects
    /// matter; the property type was built from the annotation in `fill_class`).
    ///
    /// M13: a **static** member's body (a `static` method body, or a static field
    /// initializer) is checked **too** — statics are real members now, so leaving their
    /// bodies unchecked would be a false negative (a bad return / unresolved name / wrong
    /// call inside a static body would be silently accepted). A static body's `this` is
    /// the **static side** (the class value, [`ClassInfo::static_side`]), not an instance;
    /// it is bound per static member and restored after, so it does not leak into a
    /// following instance member. `current_class` stays the class throughout, so
    /// same-class static member access is allowed.
    ///
    /// M12: the **base constructor signature** ([`ClassInfo::super_ctor`]) is bound to
    /// [`Pass::current_super_ctor`] for the same duration (same save/restore), so a
    /// `super(args)` call inside the derived constructor body is checked against it
    /// ([`infer_call`]). It is `None` for a class with no `extends`, and never leaks.
    ///
    /// M15: a **getter/setter** body is a `MethodDefinition` like any method, so it is
    /// checked here through the same [`infer_function`] path — the getter body's `return`
    /// is checked against its declared return type (`TK2322`), the setter's parameter binds
    /// in its own scope, and `this` is the instance type — with no accessor-specific code.
    /// An **abstract method** (`abstract m(): T;`) has no body; `infer_function` simply walks
    /// nothing for it (its property type was already built in `fill_class`).
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

        // Save/restore `this`, the class context, and the base-constructor signature
        // around the class so none leak (and a nested class's own values do not clobber an
        // enclosing one permanently).
        let saved_this = self.current_this;
        let saved_class = self.current_class;
        let saved_super_ctor = self.current_super_ctor;
        // M14: a class body is not itself a constructor — reset the in-constructor flag on
        // entry (it is set `true` per-member only for the `constructor` body below) and
        // restore it on exit, so a class declared inside an outer constructor body does not
        // wrongly mark this class's members as constructor-scoped.
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

        // M16: push the class's type-parameter frame for the whole body-checking pass, so a
        // method/constructor body annotation referencing `T` (a parameter or return type)
        // resolves to the same parameter type the instance template embeds. The frame is
        // rebuilt from the class's stored ids (allocated once in phase 0) paired with the
        // AST names — re-interning a `TypeParam` is idempotent, so this yields the identical
        // ids. An empty frame for a non-generic class (harmless). The frame is popped at the
        // end (a parameter never leaks past the class body).
        let type_params = value_decl
            .and_then(|decl_id| self.class_type_params.get(&decl_id).cloned())
            .unwrap_or_default();
        let frame = self.build_type_param_frame(class.type_parameters.as_deref(), &type_params);

        // M16: check every member body with the type-parameter frame pushed, so a body
        // annotation referencing `T` resolves. An empty frame for a non-generic class — then
        // this is exactly the M11–M15 body-checking pass.
        self.with_type_params(frame, |pass| {
            pass.check_class_member_bodies(scope, class, static_this)
        });

        self.current_this = saved_this;
        self.current_class = saved_class;
        self.current_super_ctor = saved_super_ctor;
        self.current_in_ctor = saved_in_ctor;
    }

    /// Check every member **body** of a class (M11–M16), with `this`/`current_class`/
    /// `current_super_ctor` already bound by the caller ([`check_class`]) and any M16
    /// type-parameter frame already pushed. Extracted so the caller can run it inside
    /// [`with_type_params`] without a deeply-indented closure; it walks the bodies and
    /// collects their obligations/diagnostics, returning nothing.
    ///
    /// A field initializer is walked (with a `static` initializer's `this` bound to the
    /// static side, M13); a method/constructor/accessor body is checked via the shared
    /// [`infer_function`] machine, with a `static` method's `this` bound to the static side
    /// and the `constructor` body flagged for the M14 `readonly`-assignment carve-out. Each
    /// per-member `this`/in-constructor override is saved/restored so it does not leak to the
    /// next member.
    fn check_class_member_bodies(
        &mut self,
        scope: ScopeId,
        class: &Class<'_>,
        static_this: Option<TypeId>,
    ) {
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                // A field initializer expression is walked here so it is checked (and
                // resolves `this`). M13: a **static** field initializer is checked too,
                // with `this` bound to the static side (the class value). Static blocks and
                // `accessor`-property declarations remain out of subset. (M15 `get`/`set`
                // accessors are `MethodDefinition`s, so they take the arm below.)
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
                        self.infer_expr(scope, init);
                        self.current_this = saved_member_this;
                    }
                }
                continue;
            };
            // Check the method/constructor body via the shared function machine. The
            // returned function type is discarded (the property type was already built in
            // `fill_class`); we keep only the body-checking side effects (return
            // obligations, nested-construct resolution, name resolution) under the bound
            // `this`.
            //
            // M13: a **static** method body is checked exactly like an instance method,
            // except `this` is bound to the **static side** (`this` inside a static method
            // is the class value, not an instance). `current_class` stays the class, so
            // same-class static access is allowed; it is saved/restored per static member
            // so a static body does not leak its `this` to a following instance member.
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

    /// Apply M13 access control to a member access `obj.m` whose member was found on the
    /// base type. A `public` member is always reachable. A `private` member is reachable
    /// only when the accessing context ([`Pass::current_class`]) **is** the member's
    /// declaring class (else `TK2341`). A `protected` member is reachable when the
    /// context is the declaring class **or a subclass** of it (else `TK2445`).
    ///
    /// The rule keys on the **declaring class**, not the instance, so same-class access to
    /// *another* instance's `private` member is allowed (the context matches the origin).
    /// An access outside any class member (`current_class == None`) matches no origin, so
    /// a non-public member is correctly rejected there. A member with no declaring class
    /// (an ordinary object/interface member — always `Public`) never reaches the
    /// non-public arms.
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

    /// Whether `class_id` **is** `ancestor` or a **subclass** of it (M13), by walking the
    /// [`Pass::class_parents`] chain upward from `class_id`. Used to decide `protected`
    /// access: the accessing context's class must be the declaring class or a descendant.
    /// The walk is bounded by a visited set so a malformed `extends` cycle terminates
    /// (cyclic hierarchies are invalid TS but must not hang).
    fn is_class_or_subclass(&self, class_id: ClassId, ancestor: ClassId) -> bool {
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

}

/// Build the **accessor properties** for a class from its accumulated getters/setters
/// (M15), stamping each with the class's [`ClassId`]. Each accessor becomes **one**
/// property:
///
///  - **type** = the getter's return type (a getter must exist — see below);
///  - **`readonly`** = a getter exists **and** there is no setter (get-only → behaves
///    like a read-only property, reusing the M14 flag); a get+set pair is writable.
///
/// A **set-only** accessor (no getter) is **deferred**: it builds no property (so the
/// name is simply absent from the instance type — its body is still walked by
/// `check_class`). The spec's "else the setter's parameter type" type fallback is part of
/// that deferral and not taken here. An accessor whose getter return type could not be
/// lowered (out of subset) is skipped too. The result reuses the ordinary
/// [`PropertyType`], so member read resolves the property type and member assignment
/// reuses the M14 path (`TK2322`/`TK2540`) with no accessor-specific machinery.
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

/// Lower an AST `accessibility` modifier ([`TSAccessibility`]) to a [`Visibility`]
/// (M13). An absent modifier (`None`) is `public` (the default — no diagnostics).
fn lower_visibility(accessibility: Option<TSAccessibility>) -> Visibility {
    match accessibility {
        Some(TSAccessibility::Private) => Visibility::Private,
        Some(TSAccessibility::Protected) => Visibility::Protected,
        Some(TSAccessibility::Public) | None => Visibility::Public,
    }
}

/// Compose a derived class's instance members from its **base** members and its
/// **own** (M12). The base's members come first; each of the class's own members
/// **overrides** a base member of the same name (the derived type wins) or is appended
/// when the name is new. This yields a width-superset of the base, so subclass→base
/// assignability falls out of the structural relation (the base is width-narrower).
///
/// The interner re-sorts into canonical (name-sorted) order when the object is filled,
/// so the order here only affects which entry survives a duplicate name — own over
/// base, matching TS override semantics. (Override *compatibility* checking, `TK2416`,
/// is deferred: an incompatible override is accepted, it just replaces the base type.)
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

