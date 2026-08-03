//! Stable full-library identities used by native-syntax member projection.

use super::context::{Pass, TypeDecl};
use super::type_groups::{
    PublishedTypeEnvironment, PublishedTypeGroupSurface, PublishedTypeGroupTerminal,
};
use crate::binder::declaration::TypeGroupId;
use crate::binder::roots::LibraryRootProjection;
use crate::binder::scope::ScopeId;
use crate::binder::Binder;
use crate::check::query::{
    classify_native_source_surface, LibraryObjectRelationContext, NativeSourceSurface,
};
use crate::span::Span;
use crate::types::repr::{ClassId, LiteralValue, TypeFlags, TypeParamId, TypeTag, Visibility};
use crate::types::store::{Store, TypeId};
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static FORCE_COLLISION_IDENTITY_SELECTION_PENDING: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LibraryIdentityUnavailable {
    MissingGlobal,
    Unpublished,
    UnsupportedSurface,
    WrongArity,
    ContainsError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LibraryTypeIdentity {
    pub(crate) group: TypeGroupId,
    pub(crate) template: TypeId,
    pub(crate) parameters: Vec<TypeParamId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LibraryIdentityTerminal {
    Ready(LibraryTypeIdentity),
    Unavailable(LibraryIdentityUnavailable),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LibrarySemanticIdentities {
    inner: Arc<LibrarySemanticIdentityRows>,
}

#[derive(Debug, PartialEq, Eq)]
struct LibrarySemanticIdentityRows {
    array: LibraryIdentityTerminal,
    readonly_array: LibraryIdentityTerminal,
    string: LibraryIdentityTerminal,
    number: LibraryIdentityTerminal,
    boolean: LibraryIdentityTerminal,
    regexp: LibraryIdentityTerminal,
    object: LibraryIdentityTerminal,
    callable_function: LibraryIdentityTerminal,
}

pub(crate) type LibrarySemanticIdentitiesProductParts = [LibraryIdentityTerminal; 8];

/// Which native array syntax a library type group *is*. `Array<T>` and
/// `ReadonlyArray<T>` name the intrinsic array types themselves; their interface
/// bodies are only the member surface `project_library_member_surface` projects.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum NativeArrayAlias {
    Array,
    ReadonlyArray,
}

/// The two native-array declaration identities, carried by value so annotation
/// lowering can test group identity without borrowing the identity table.
///
/// Keyed on the universe-local `TypeGroupId` selected from the library's
/// compilation-global scope — never on the spelling — so a module-local
/// `interface Array<T>` of the user's own acquires no native role.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct NativeArrayGroups {
    array: Option<TypeGroupId>,
    readonly_array: Option<TypeGroupId>,
}

impl NativeArrayGroups {
    pub(in crate::check::checker) fn alias_of(
        self,
        group: TypeGroupId,
    ) -> Option<NativeArrayAlias> {
        if self.array == Some(group) {
            return Some(NativeArrayAlias::Array);
        }
        if self.readonly_array == Some(group) {
            return Some(NativeArrayAlias::ReadonlyArray);
        }
        None
    }
}

impl LibrarySemanticIdentities {
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn normalized_projection_for_test(
        &self,
        store: &Store,
    ) -> Vec<String> {
        [
            ("Array", &self.inner.array),
            ("ReadonlyArray", &self.inner.readonly_array),
            ("String", &self.inner.string),
            ("Number", &self.inner.number),
            ("Boolean", &self.inner.boolean),
            ("RegExp", &self.inner.regexp),
            ("Object", &self.inner.object),
            ("CallableFunction", &self.inner.callable_function),
        ]
        .into_iter()
        .map(|(name, terminal)| match terminal {
            LibraryIdentityTerminal::Ready(identity) => format!(
                "{name}:{}:{}",
                identity.parameters.len(),
                crate::diagnostics::render_type(store, identity.template, false)
            ),
            LibraryIdentityTerminal::Unavailable(cause) => {
                format!("{name}:unavailable:{cause:?}")
            }
        })
        .collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn group_for_name_for_test(
        &self,
        name: &str,
    ) -> Option<TypeGroupId> {
        let terminal = match name {
            "Array" => &self.inner.array,
            "ReadonlyArray" => &self.inner.readonly_array,
            "String" => &self.inner.string,
            "Number" => &self.inner.number,
            "Boolean" => &self.inner.boolean,
            "RegExp" => &self.inner.regexp,
            "Object" => &self.inner.object,
            "CallableFunction" => &self.inner.callable_function,
            _ => return None,
        };
        match terminal {
            LibraryIdentityTerminal::Ready(identity) => Some(identity.group),
            LibraryIdentityTerminal::Unavailable(_) => None,
        }
    }

    /// Select roots once from the library compilation-global scope. Consumer scopes
    /// never participate, so same-named user declarations cannot hijack native syntax.
    pub(in crate::check::checker) fn select(
        binder: &Binder,
        published: &PublishedTypeEnvironment,
        store: &Store,
    ) -> Self {
        Self::select_from_scope(binder, binder.compilation_global, published, store)
    }

    pub(in crate::check::checker) fn select_for_collision_publication(
        binder: &Binder,
        published: &PublishedTypeEnvironment,
        store: &Store,
    ) -> Self {
        let selected = Self::select(binder, published, store);
        #[cfg(any(test, feature = "test-utils"))]
        if FORCE_COLLISION_IDENTITY_SELECTION_PENDING.get() {
            let mut parts = selected.product_parts();
            parts[0] =
                LibraryIdentityTerminal::Unavailable(LibraryIdentityUnavailable::Unpublished);
            return Self::from_explicit_parts(parts);
        }
        selected
    }

    pub(in crate::check::checker) fn select_from_library_roots(
        projection: &LibraryRootProjection,
        published: &PublishedTypeEnvironment,
        store: &Store,
    ) -> Self {
        let select = |name: &str, arity| {
            let group = projection
                .root_rows
                .iter()
                .find(|row| row.name == name)
                .and_then(|row| row.ty);
            select_exact_group(published, store, group, arity)
        };
        Self {
            inner: Arc::new(LibrarySemanticIdentityRows {
                array: select("Array", 1),
                readonly_array: select("ReadonlyArray", 1),
                string: select("String", 0),
                number: select("Number", 0),
                boolean: select("Boolean", 0),
                regexp: select("RegExp", 0),
                object: select("Object", 0),
                callable_function: select("CallableFunction", 0),
            }),
        }
    }

    pub(in crate::check::checker) fn select_from_scope(
        binder: &Binder,
        scope: ScopeId,
        published: &PublishedTypeEnvironment,
        store: &Store,
    ) -> Self {
        Self {
            inner: Arc::new(LibrarySemanticIdentityRows {
                array: select_in_scope(binder, scope, published, store, "Array", 1),
                readonly_array: select_in_scope(
                    binder,
                    scope,
                    published,
                    store,
                    "ReadonlyArray",
                    1,
                ),
                string: select_in_scope(binder, scope, published, store, "String", 0),
                number: select_in_scope(binder, scope, published, store, "Number", 0),
                boolean: select_in_scope(binder, scope, published, store, "Boolean", 0),
                regexp: select_in_scope(binder, scope, published, store, "RegExp", 0),
                object: select_in_scope(binder, scope, published, store, "Object", 0),
                callable_function: select_in_scope(
                    binder,
                    scope,
                    published,
                    store,
                    "CallableFunction",
                    0,
                ),
            }),
        }
    }

    pub(crate) fn all_ready(&self) -> bool {
        self.terminals()
            .into_iter()
            .all(|terminal| matches!(terminal, LibraryIdentityTerminal::Ready(_)))
    }

    pub(crate) fn terminals(&self) -> [&LibraryIdentityTerminal; 8] {
        [
            &self.inner.array,
            &self.inner.readonly_array,
            &self.inner.string,
            &self.inner.number,
            &self.inner.boolean,
            &self.inner.regexp,
            &self.inner.object,
            &self.inner.callable_function,
        ]
    }

    pub(in crate::check::checker) fn array_group(&self) -> Option<TypeGroupId> {
        match &self.inner.array {
            LibraryIdentityTerminal::Ready(identity) => Some(identity.group),
            LibraryIdentityTerminal::Unavailable(_) => None,
        }
    }

    fn readonly_array_group(&self) -> Option<TypeGroupId> {
        match &self.inner.readonly_array {
            LibraryIdentityTerminal::Ready(identity) => Some(identity.group),
            LibraryIdentityTerminal::Unavailable(_) => None,
        }
    }

    pub(in crate::check::checker) fn native_array_groups(&self) -> NativeArrayGroups {
        NativeArrayGroups {
            array: self.array_group(),
            readonly_array: self.readonly_array_group(),
        }
    }

    pub(in crate::check::checker) fn array_definitely_lacks_then(&self, store: &Store) -> bool {
        [&self.inner.array, &self.inner.object]
            .into_iter()
            .all(|terminal| {
                let LibraryIdentityTerminal::Ready(identity) = terminal else {
                    return false;
                };
                store.object_type(identity.template).is_some_and(|object| {
                    object.property("then").is_none() && object.string_index.is_none()
                })
            })
    }

    pub(in crate::check::checker) fn object_relation_context(
        &self,
    ) -> Option<LibraryObjectRelationContext> {
        let LibraryIdentityTerminal::Ready(array) = &self.inner.array else {
            return None;
        };
        let LibraryIdentityTerminal::Ready(readonly_array) = &self.inner.readonly_array else {
            return None;
        };
        let LibraryIdentityTerminal::Ready(string) = &self.inner.string else {
            return None;
        };
        let LibraryIdentityTerminal::Ready(number) = &self.inner.number else {
            return None;
        };
        let LibraryIdentityTerminal::Ready(boolean) = &self.inner.boolean else {
            return None;
        };
        let LibraryIdentityTerminal::Ready(object) = &self.inner.object else {
            return None;
        };
        let LibraryIdentityTerminal::Ready(callable_function) = &self.inner.callable_function
        else {
            return None;
        };
        let [array_parameter] = array.parameters.as_slice() else {
            return None;
        };
        let [readonly_array_parameter] = readonly_array.parameters.as_slice() else {
            return None;
        };
        Some(LibraryObjectRelationContext::new(
            object.template,
            (array.template, *array_parameter),
            (readonly_array.template, *readonly_array_parameter),
            string.template,
            number.template,
            boolean.template,
            callable_function.template,
        ))
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn callable_function_group(&self) -> Option<TypeGroupId> {
        match &self.inner.callable_function {
            LibraryIdentityTerminal::Ready(identity) => Some(identity.group),
            LibraryIdentityTerminal::Unavailable(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_explicit_for_test(terminals: [LibraryIdentityTerminal; 8]) -> Self {
        Self::from_explicit_parts(terminals)
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn from_explicit_parts(terminals: [LibraryIdentityTerminal; 8]) -> Self {
        let [array, readonly_array, string, number, boolean, regexp, object, callable_function] =
            terminals;
        Self {
            inner: Arc::new(LibrarySemanticIdentityRows {
                array,
                readonly_array,
                string,
                number,
                boolean,
                regexp,
                object,
                callable_function,
            }),
        }
    }

    pub(crate) fn product_parts(&self) -> LibrarySemanticIdentitiesProductParts {
        [
            self.inner.array.clone(),
            self.inner.readonly_array.clone(),
            self.inner.string.clone(),
            self.inner.number.clone(),
            self.inner.boolean.clone(),
            self.inner.regexp.clone(),
            self.inner.object.clone(),
            self.inner.callable_function.clone(),
        ]
    }

    #[cfg(test)]
    pub(crate) fn from_product_parts(
        terminals: LibrarySemanticIdentitiesProductParts,
    ) -> Result<Self, &'static str> {
        for terminal in &terminals {
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                continue;
            };
            let mut parameters = std::collections::BTreeSet::new();
            if identity
                .parameters
                .iter()
                .any(|parameter| !parameters.insert(*parameter))
            {
                return Err("restored semantic identity repeats a parameter id");
            }
        }
        Ok(Self::from_explicit_for_test(terminals))
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn with_forced_collision_identity_selection_pending<T>(run: impl FnOnce() -> T) -> T {
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            FORCE_COLLISION_IDENTITY_SELECTION_PENDING.set(false);
        }
    }

    FORCE_COLLISION_IDENTITY_SELECTION_PENDING.set(true);
    let reset = Reset;
    let result = run();
    drop(reset);
    result
}

fn select_in_scope(
    binder: &Binder,
    scope: ScopeId,
    published: &PublishedTypeEnvironment,
    store: &Store,
    name: &str,
    arity: usize,
) -> LibraryIdentityTerminal {
    let group = binder
        .graph
        .get(scope)
        .and_then(|scope| scope.lookup_local(name))
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty);
    select_exact_group(published, store, group, arity)
}

fn select_exact_group(
    published: &PublishedTypeEnvironment,
    store: &Store,
    group: Option<TypeGroupId>,
    arity: usize,
) -> LibraryIdentityTerminal {
    let Some(group) = group else {
        return LibraryIdentityTerminal::Unavailable(LibraryIdentityUnavailable::MissingGlobal);
    };
    let Some(terminal) = published.groups().get(group) else {
        return LibraryIdentityTerminal::Unavailable(LibraryIdentityUnavailable::Unpublished);
    };
    let PublishedTypeGroupTerminal::Ready(published) = terminal else {
        return LibraryIdentityTerminal::Unavailable(LibraryIdentityUnavailable::Unpublished);
    };
    if published.parameters.len() != arity {
        return LibraryIdentityTerminal::Unavailable(LibraryIdentityUnavailable::WrongArity);
    }
    let PublishedTypeGroupSurface::Template(template) = published.surface else {
        return LibraryIdentityTerminal::Unavailable(
            LibraryIdentityUnavailable::UnsupportedSurface,
        );
    };
    if store.flags(template).contains(TypeFlags::CONTAINS_ERROR) {
        return LibraryIdentityTerminal::Unavailable(LibraryIdentityUnavailable::ContainsError);
    }
    LibraryIdentityTerminal::Ready(LibraryTypeIdentity {
        group,
        template,
        parameters: published.parameters.clone(),
    })
}

pub(in crate::check::checker) enum LibraryMemberProjection {
    NotApplicable,
    Ready(TypeId),
    Unavailable,
}

pub(in crate::check::checker) enum LibraryComposedMember {
    NotInstalled,
    Ready {
        ty: TypeId,
        access: Vec<(Visibility, Option<ClassId>)>,
    },
    Missing,
    Unavailable,
}

enum NativeMemberBridge {
    Generic {
        terminal: LibraryIdentityTerminal,
        argument: TypeId,
        incomplete_id: &'static str,
    },
    Plain {
        terminal: LibraryIdentityTerminal,
        incomplete_id: &'static str,
    },
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    pub(in crate::check::checker) fn install_library_semantic_identities(
        &mut self,
        identities: LibrarySemanticIdentities,
    ) {
        assert!(self.library_semantic_identities.is_none());
        self.semantic_queries
            .set_library_object_context(identities.object_relation_context());
        self.library_semantic_identities = Some(identities);
    }

    pub(in crate::check::checker) fn native_member_length(&mut self, ty: TypeId) -> Option<TypeId> {
        let ty = self.interner.store().readonly_operand(ty).unwrap_or(ty);
        if self.interner.store().array_type(ty).is_some() {
            return Some(self.interner.well_known().number);
        }
        let tuple = self.interner.store().tuple_type(ty)?;
        if tuple.rest.is_some() {
            return Some(self.interner.well_known().number);
        }
        let length = tuple.elements.len() as f64;
        Some(self.interner.intern_literal(LiteralValue::Number(length)))
    }

    pub(in crate::check::checker) fn project_library_member_surface(
        &mut self,
        ty: TypeId,
        span: Span,
    ) -> LibraryMemberProjection {
        let Some(identities) = self.library_semantic_identities.clone() else {
            return LibraryMemberProjection::NotApplicable;
        };
        let bridge = match native_bridge(self.interner, ty, &identities) {
            Some(bridge) => bridge,
            None => return LibraryMemberProjection::NotApplicable,
        };
        let (terminal, argument, incomplete_id) = match bridge {
            NativeMemberBridge::Generic {
                terminal,
                argument,
                incomplete_id,
            } => (terminal, Some(argument), incomplete_id),
            NativeMemberBridge::Plain {
                terminal,
                incomplete_id,
            } => (terminal, None, incomplete_id),
        };
        let LibraryIdentityTerminal::Ready(identity) = terminal else {
            self.record_incomplete(
                incomplete_id,
                span,
                "native member surface is unavailable from the installed library",
            );
            return LibraryMemberProjection::Unavailable;
        };
        if (matches!(self.current_source, crate::source::SourceUnit::User { .. })
            || self.combined_user_source)
            && self
                .private_collision_unavailable_type_groups
                .contains(&identity.group)
        {
            self.record_incomplete(
                incomplete_id,
                span,
                "native member surface is unavailable in the private collision epoch",
            );
            return LibraryMemberProjection::Unavailable;
        }
        let projected = match argument {
            Some(argument) => {
                let Some(parameter) = identity.parameters.first().copied() else {
                    self.record_incomplete(
                        incomplete_id,
                        span,
                        "native member surface has an invalid generic identity",
                    );
                    return LibraryMemberProjection::Unavailable;
                };
                let substitutions = FxHashMap::from_iter([(parameter, argument)]);
                self.substitute_ready_type_group_application(
                    identity.template,
                    &identity.parameters,
                    &substitutions,
                )
            }
            None => identity.template,
        };
        LibraryMemberProjection::Ready(projected)
    }

    pub(in crate::check::checker) fn project_library_heritage_surface(
        &mut self,
        scope: ScopeId,
        ty: TypeId,
    ) -> Option<TypeId> {
        let identities = self.library_semantic_identities.clone()?;
        let NativeMemberBridge::Generic {
            terminal, argument, ..
        } = native_bridge(self.interner, ty, &identities)?
        else {
            return None;
        };
        let LibraryIdentityTerminal::Ready(identity) = terminal else {
            return None;
        };
        // A private collision rebuild publishes after interface construction. Read its
        // exact frozen replacement here instead of the still-installed base template.
        let replacement = (!self.type_environment.is_published()
            && self.type_decls.has_replacement(identity.group.index())
            && self.type_group_construction_is_frozen(identity.group))
        .then(|| match self.type_decls.get(identity.group.index()) {
            Some(TypeDecl::Interface {
                recovery_params, ..
            }) => Some(recovery_params.clone()),
            _ => None,
        })
        .flatten();
        let (template, parameters) = if let Some(parameters) = replacement {
            (self.resolve_type_decl(scope, identity.group), parameters)
        } else {
            (identity.template, identity.parameters.clone())
        };
        let parameter = parameters.first().copied()?;
        let substitutions = FxHashMap::from_iter([(parameter, argument)]);
        Some(self.substitute_ready_type_group_application(template, &parameters, &substitutions))
    }

    pub(in crate::check::checker) fn lookup_library_composed_member(
        &mut self,
        ty: TypeId,
        property: &str,
        span: Span,
    ) -> LibraryComposedMember {
        #[cfg(any(test, feature = "test-utils"))]
        super::library_compiler::record_private_replay_visibility_query_for_test();
        if self.library_semantic_identities.is_none() {
            return LibraryComposedMember::NotInstalled;
        }
        self.lookup_library_composed_member_inner(ty, property, span)
    }

    fn lookup_library_composed_member_inner(
        &mut self,
        ty: TypeId,
        property: &str,
        span: Span,
    ) -> LibraryComposedMember {
        let ty = match self.demand_apparent_type(ty) {
            crate::class_semantics::DemandOutcome::Ready(ty) => ty,
            crate::class_semantics::DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(
                    crate::class_semantics::DemandOutcome::Exhausted(exhaustion),
                    span,
                );
                return LibraryComposedMember::Unavailable;
            }
        };
        let tag = self.interner.store().tag(ty);
        if matches!(tag, TypeTag::Union | TypeTag::Intersection) {
            let members = if tag == TypeTag::Union {
                self.interner.store().union_members(ty)
            } else {
                self.interner.store().intersection_members(ty)
            }
            .map(|members| members.to_vec())
            .unwrap_or_default();
            let mut results = Vec::with_capacity(members.len());
            let mut access = Vec::new();
            for member in members {
                match self.lookup_library_composed_member_inner(member, property, span) {
                    LibraryComposedMember::Ready {
                        ty,
                        access: member_access,
                    } => {
                        results.push(ty);
                        for constraint in member_access {
                            if !access.contains(&constraint) {
                                access.push(constraint);
                            }
                        }
                    }
                    LibraryComposedMember::Missing if tag == TypeTag::Intersection => {}
                    LibraryComposedMember::Missing => return LibraryComposedMember::Missing,
                    LibraryComposedMember::Unavailable => {
                        return LibraryComposedMember::Unavailable;
                    }
                    LibraryComposedMember::NotInstalled => unreachable!(),
                }
            }
            if results.is_empty() {
                return LibraryComposedMember::Missing;
            }
            let ty = if tag == TypeTag::Union {
                self.interner.union(results)
            } else {
                self.interner.intersection(results)
            };
            return LibraryComposedMember::Ready { ty, access };
        }

        if property == "length" {
            if let Some(length) = self.native_member_length(ty) {
                return LibraryComposedMember::Ready {
                    ty: length,
                    access: Vec::new(),
                };
            }
        }

        let lookup_ty = match self.project_library_member_surface(ty, span) {
            LibraryMemberProjection::Ready(projected) => projected,
            LibraryMemberProjection::Unavailable => return LibraryComposedMember::Unavailable,
            LibraryMemberProjection::NotApplicable => {
                match self.demand_structural_apparent_type(ty) {
                    crate::class_semantics::DemandOutcome::Ready(apparent) => apparent,
                    crate::class_semantics::DemandOutcome::Exhausted(exhaustion) => {
                        self.own_type_demand(
                            crate::class_semantics::DemandOutcome::Exhausted(exhaustion),
                            span,
                        );
                        return LibraryComposedMember::Unavailable;
                    }
                }
            }
        };

        if let Some(member) = direct_object_member(self.interner.store(), lookup_ty, property) {
            return member;
        }
        if self.interner.store().tag(lookup_ty) != TypeTag::Object {
            return LibraryComposedMember::Missing;
        }

        let terminal = self
            .library_semantic_identities
            .as_ref()
            .expect("installed identities remain available")
            .inner
            .object
            .clone();
        let LibraryIdentityTerminal::Ready(identity) = terminal else {
            self.record_incomplete(
                "library-bridge/object/identity",
                span,
                "Object identity is unavailable from the installed library",
            );
            return LibraryComposedMember::Unavailable;
        };
        direct_object_member(self.interner.store(), identity.template, property)
            .unwrap_or(LibraryComposedMember::Missing)
    }

    pub(in crate::check::checker) fn regexp_literal_type(&mut self, span: Span) -> Option<TypeId> {
        let identities = self.library_semantic_identities.as_ref()?;
        match &identities.inner.regexp {
            LibraryIdentityTerminal::Ready(identity) => Some(identity.template),
            LibraryIdentityTerminal::Unavailable(_) => {
                self.record_incomplete(
                    "library-bridge/regexp-literal/identity",
                    span,
                    "RegExp identity is unavailable from the installed library",
                );
                Some(self.interner.well_known().error)
            }
        }
    }
}

fn direct_object_member(
    store: &Store,
    ty: TypeId,
    property: &str,
) -> Option<LibraryComposedMember> {
    let object = store.object_type(ty)?;
    if let Some(member) = object.property(property) {
        return Some(LibraryComposedMember::Ready {
            ty: member.ty,
            access: vec![(member.visibility, member.declaring_class)],
        });
    }
    object.string_index.map(|ty| LibraryComposedMember::Ready {
        ty,
        access: Vec::new(),
    })
}

fn native_bridge(
    interner: &mut crate::types::Interner,
    ty: TypeId,
    identities: &LibrarySemanticIdentities,
) -> Option<NativeMemberBridge> {
    match classify_native_source_surface(interner, ty)? {
        NativeSourceSurface::Array {
            readonly, argument, ..
        } => Some(NativeMemberBridge::Generic {
            terminal: if readonly {
                identities.inner.readonly_array.clone()
            } else {
                identities.inner.array.clone()
            },
            argument,
            incomplete_id: if interner
                .store()
                .tuple_type(interner.store().readonly_operand(ty).unwrap_or(ty))
                .is_some()
            {
                "library-bridge/tuple/identity"
            } else {
                "library-bridge/array/identity"
            },
        }),
        NativeSourceSurface::String => Some(NativeMemberBridge::Plain {
            terminal: identities.inner.string.clone(),
            incomplete_id: "library-bridge/string/identity",
        }),
        NativeSourceSurface::Number => Some(NativeMemberBridge::Plain {
            terminal: identities.inner.number.clone(),
            incomplete_id: "library-bridge/number/identity",
        }),
        NativeSourceSurface::Boolean => Some(NativeMemberBridge::Plain {
            terminal: identities.inner.boolean.clone(),
            incomplete_id: "library-bridge/boolean/identity",
        }),
        NativeSourceSurface::Object => Some(NativeMemberBridge::Plain {
            terminal: identities.inner.object.clone(),
            incomplete_id: "library-bridge/object/identity",
        }),
        NativeSourceSurface::Function => Some(NativeMemberBridge::Plain {
            terminal: identities.inner.callable_function.clone(),
            incomplete_id: "library-bridge/callable-function/identity",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::bind_module_with_prelude;
    use crate::check::checker::{build_pass, context::DeclTypes};
    use crate::check::test_support::check_source;
    use crate::diagnostics::DiagnosticCode;
    use crate::types::repr::{ObjectType, PropertyType};
    use crate::types::Interner;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn ready(
        group: u32,
        template: TypeId,
        parameters: Vec<TypeParamId>,
    ) -> LibraryIdentityTerminal {
        LibraryIdentityTerminal::Ready(LibraryTypeIdentity {
            group: TypeGroupId(group),
            template,
            parameters,
        })
    }

    fn object(interner: &mut Interner, properties: &[(&str, TypeId)]) -> TypeId {
        interner.intern_object(ObjectType {
            properties: properties
                .iter()
                .map(|(name, ty)| PropertyType::public(*name, *ty))
                .collect(),
            ..Default::default()
        })
    }

    fn access_object(
        interner: &mut Interner,
        name: &str,
        ty: TypeId,
        visibility: Visibility,
        declaring_class: Option<ClassId>,
        discriminator: Option<(&str, TypeId)>,
    ) -> TypeId {
        let mut member = PropertyType::public(name, ty);
        member.visibility = visibility;
        member.declaring_class = declaring_class;
        let mut properties = vec![member];
        if let Some((name, ty)) = discriminator {
            properties.push(PropertyType::public(name, ty));
        }
        interner.intern_object(ObjectType {
            properties,
            ..Default::default()
        })
    }

    fn with_bridge_pass(test: impl FnOnce(&mut Pass<'_, '_>, &LibrarySemanticIdentities)) {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, "", SourceType::ts()).parse();
        let binder = bind_module_with_prelude(&parsed.program, &parsed.program);
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let array_parameter = TypeParamId(100);
        let parameter_ty = interner.intern_type_param(array_parameter, "T");
        let array = object(
            &mut interner,
            &[("mapResult", parameter_ty), ("pushValue", parameter_ty)],
        );
        let readonly_array = object(&mut interner, &[("mapResult", parameter_ty)]);
        let string = object(
            &mut interner,
            &[
                ("toUpperCaseResult", wk.string),
                ("valueOfResult", wk.string),
            ],
        );
        let number = object(
            &mut interner,
            &[("toFixedResult", wk.string), ("valueOfResult", wk.number)],
        );
        let boolean = object(&mut interner, &[("valueOfResult", wk.boolean)]);
        let regexp = object(&mut interner, &[("testResult", wk.boolean)]);
        let object_surface = object(&mut interner, &[("toStringResult", wk.string)]);
        let callable_function = object(&mut interner, &[("callResult", wk.number)]);
        let identities = LibrarySemanticIdentities::from_explicit_for_test([
            ready(0, array, vec![array_parameter]),
            ready(1, readonly_array, vec![array_parameter]),
            ready(2, string, Vec::new()),
            ready(3, number, Vec::new()),
            ready(4, boolean, Vec::new()),
            ready(5, regexp, Vec::new()),
            ready(6, object_surface, Vec::new()),
            ready(7, callable_function, Vec::new()),
        ]);
        let decl_count = binder.decl_count;
        let mut pass = build_pass(
            &mut interner,
            &binder,
            Vec::new(),
            Vec::new(),
            DeclTypes::new(decl_count),
            0,
        );
        pass.publish_class_surfaces();
        pass.publish_type_groups();
        pass.install_library_semantic_identities(identities.clone());
        test(&mut pass, &identities);
    }

    fn projected_property(pass: &mut Pass<'_, '_>, ty: TypeId, property: &str) -> Option<TypeId> {
        let LibraryMemberProjection::Ready(projected) =
            pass.project_library_member_surface(ty, Span::new(0, 0))
        else {
            panic!("native type must project through a ready identity")
        };
        pass.interner
            .store()
            .object_type(projected)
            .and_then(|object| object.property(property))
            .map(|property| property.ty)
    }

    fn composed_property(pass: &mut Pass<'_, '_>, ty: TypeId, property: &str) -> Option<TypeId> {
        match pass.lookup_library_composed_member(ty, property, Span::new(0, 0)) {
            LibraryComposedMember::Ready { ty, .. } => Some(ty),
            LibraryComposedMember::Missing => None,
            LibraryComposedMember::Unavailable => panic!("synthetic identity is ready"),
            LibraryComposedMember::NotInstalled => panic!("synthetic identities are installed"),
        }
    }

    fn composed_access(
        pass: &mut Pass<'_, '_>,
        ty: TypeId,
        property: &str,
    ) -> Vec<(Visibility, Option<ClassId>)> {
        match pass.lookup_library_composed_member(ty, property, Span::new(0, 0)) {
            LibraryComposedMember::Ready { access, .. } => access,
            LibraryComposedMember::Missing => panic!("synthetic member exists"),
            LibraryComposedMember::Unavailable => panic!("synthetic identity is ready"),
            LibraryComposedMember::NotInstalled => panic!("synthetic identities are installed"),
        }
    }

    #[test]
    fn native_collection_projection_preserves_element_and_readonly_semantics() {
        with_bridge_pass(|pass, _| {
            let wk = pass.interner.well_known();
            let array = pass.interner.intern_array(wk.number);
            assert_eq!(
                projected_property(pass, array, "mapResult"),
                Some(wk.number)
            );
            assert_eq!(
                projected_property(pass, array, "pushValue"),
                Some(wk.number)
            );

            let readonly = pass.interner.intern_readonly(array);
            assert_eq!(
                projected_property(pass, readonly, "mapResult"),
                Some(wk.number)
            );
            assert_eq!(projected_property(pass, readonly, "pushValue"), None);

            let tuple = pass.interner.intern_tuple(vec![wk.string, wk.number]);
            let tuple_element = projected_property(pass, tuple, "mapResult")
                .expect("tuple projects an Array element union");
            let expected_element = pass.interner.union(vec![wk.string, wk.number]);
            assert_eq!(
                pass.interner.store().union_members(tuple_element),
                pass.interner.store().union_members(expected_element)
            );
            assert_eq!(
                pass.native_member_length(tuple)
                    .and_then(|length| pass.interner.store().literal_value(length)),
                Some(&LiteralValue::Number(2.0))
            );
        });
    }

    #[test]
    fn object_keyword_selects_only_the_explicit_object_identity() {
        with_bridge_pass(|pass, _| {
            let wk = pass.interner.well_known();
            assert_eq!(
                projected_property(pass, wk.object, "toStringResult"),
                Some(wk.string)
            );
            assert_eq!(projected_property(pass, wk.object, "missing"), None);
        });
    }

    #[test]
    fn native_primitive_function_and_regexp_projection_uses_installed_ids() {
        with_bridge_pass(|pass, identities| {
            let wk = pass.interner.well_known();
            let text = pass
                .interner
                .intern_literal(LiteralValue::String("native".to_owned()));
            let number = pass.interner.intern_literal(LiteralValue::Number(12.5));
            let boolean = pass.interner.intern_literal(LiteralValue::Boolean(true));
            assert_eq!(
                projected_property(pass, text, "toUpperCaseResult"),
                Some(wk.string)
            );
            assert_eq!(
                projected_property(pass, number, "toFixedResult"),
                Some(wk.string)
            );
            assert_eq!(
                projected_property(pass, boolean, "valueOfResult"),
                Some(wk.boolean)
            );

            let function = crate::types::repr::FunctionType {
                type_params: Vec::new(),
                receiver: None,
                params: Vec::new(),
                ret: wk.number,
            };
            let function = pass.interner.intern_function(function);
            assert_eq!(
                projected_property(pass, function, "callResult"),
                Some(wk.number)
            );
            let expected_regexp = match &identities.inner.regexp {
                LibraryIdentityTerminal::Ready(identity) => identity.template,
                LibraryIdentityTerminal::Unavailable(_) => unreachable!(),
            };
            assert_eq!(
                pass.regexp_literal_type(Span::new(0, 0)),
                Some(expected_regexp)
            );
        });
    }

    #[test]
    fn callable_function_identity_wins_over_the_legacy_function_shape() {
        with_bridge_pass(|pass, identities| {
            let wk = pass.interner.well_known();
            let native = pass
                .interner
                .intern_function(crate::types::repr::FunctionType {
                    type_params: Vec::new(),
                    receiver: None,
                    params: Vec::new(),
                    ret: wk.number,
                });
            assert_eq!(
                composed_property(pass, native, "callResult"),
                Some(wk.number)
            );
            assert!(identities.callable_function_group().is_some());
        });
    }

    #[test]
    fn object_bridge_is_fallback_after_own_properties_and_index() {
        with_bridge_pass(|pass, _| {
            let wk = pass.interner.well_known();
            let own = object(pass.interner, &[("value", wk.number)]);
            assert_eq!(composed_property(pass, own, "value"), Some(wk.number));
            assert_eq!(
                composed_property(pass, own, "toStringResult"),
                Some(wk.string)
            );

            let override_object = object(pass.interner, &[("toStringResult", wk.number)]);
            assert_eq!(
                composed_property(pass, override_object, "toStringResult"),
                Some(wk.number)
            );

            let indexed = pass.interner.intern_object(ObjectType {
                string_index: Some(wk.boolean),
                ..Default::default()
            });
            assert_eq!(
                composed_property(pass, indexed, "toStringResult"),
                Some(wk.boolean)
            );
        });
    }

    #[test]
    fn native_composites_project_each_constituent_before_combining() {
        with_bridge_pass(|pass, _| {
            let wk = pass.interner.well_known();
            let primitive_union = pass.interner.union(vec![wk.string, wk.number]);
            let value = composed_property(pass, primitive_union, "valueOfResult")
                .expect("both primitive wrappers publish valueOfResult");
            let expected = pass.interner.union(vec![wk.string, wk.number]);
            assert_eq!(value, expected);

            let marker_value = pass.interner.intern_literal(LiteralValue::Number(1.0));
            let marker = object(pass.interner, &[("marker", marker_value)]);
            let decorated = pass.interner.intersection(vec![wk.string, marker]);
            assert_eq!(
                composed_property(pass, decorated, "toUpperCaseResult"),
                Some(wk.string)
            );
            assert_eq!(
                composed_property(pass, decorated, "marker"),
                Some(marker_value)
            );

            let missing_union = pass.interner.union(vec![wk.string, wk.boolean]);
            assert_eq!(
                composed_property(pass, missing_union, "toUpperCaseResult"),
                None
            );
        });
    }

    #[test]
    fn composite_access_constraints_are_stable_complete_and_deduplicated() {
        with_bridge_pass(|pass, _| {
            let wk = pass.interner.well_known();
            let private_owner = ClassId(40);
            let protected_owner = ClassId(41);
            let private = access_object(
                pass.interner,
                "shared",
                wk.number,
                Visibility::Private,
                Some(private_owner),
                None,
            );
            let duplicate_private = access_object(
                pass.interner,
                "shared",
                wk.number,
                Visibility::Private,
                Some(private_owner),
                Some(("distinct", wk.string)),
            );
            let protected = access_object(
                pass.interner,
                "shared",
                wk.number,
                Visibility::Protected,
                Some(protected_owner),
                None,
            );
            let public = access_object(
                pass.interner,
                "shared",
                wk.number,
                Visibility::Public,
                None,
                None,
            );

            assert_eq!(
                composed_access(pass, private, "shared"),
                [(Visibility::Private, Some(private_owner))]
            );
            assert_eq!(
                composed_access(pass, protected, "shared"),
                [(Visibility::Protected, Some(protected_owner))]
            );
            assert_eq!(
                composed_access(pass, public, "shared"),
                [(Visibility::Public, None)]
            );

            let composite = pass.interner.intersection(vec![
                private,
                duplicate_private,
                protected,
                public,
                wk.string,
            ]);
            assert_eq!(
                composed_access(pass, composite, "shared"),
                [
                    (Visibility::Private, Some(private_owner)),
                    (Visibility::Protected, Some(protected_owner)),
                    (Visibility::Public, None),
                ]
            );
        });
    }

    #[test]
    fn unavailable_native_identity_is_not_a_consumer_name_fallback() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let unavailable =
            LibraryIdentityTerminal::Unavailable(LibraryIdentityUnavailable::MissingGlobal);
        let identities = LibrarySemanticIdentities::from_explicit_for_test([
            unavailable.clone(),
            unavailable.clone(),
            unavailable.clone(),
            unavailable.clone(),
            unavailable.clone(),
            unavailable.clone(),
            unavailable.clone(),
            unavailable,
        ]);
        let native = interner.intern_array(wk.number);
        let Some(NativeMemberBridge::Generic { terminal, .. }) =
            native_bridge(&mut interner, native, &identities)
        else {
            panic!("native array must still select its fail-closed identity slot")
        };
        assert!(matches!(terminal, LibraryIdentityTerminal::Unavailable(_)));
        assert!(!identities.all_ready());
    }

    #[test]
    fn explicit_array_is_lexical_and_module_local_array_shadows_it() {
        let global =
            check_source("const ok: Array<number> = [1];\nconst wrong: Array<string> = [1];\n");
        assert!(global.parse_errors.is_empty());
        assert_eq!(
            global
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [DiagnosticCode::TK2322]
        );

        let local = check_source(
            "export {};\ninterface Array<T> { local: T }\ndeclare const value: Array<number>;\nconst ok: number = value.local;\nconst native = [1];\nnative.local;\n",
        );
        assert!(local.parse_errors.is_empty());
        assert_eq!(
            local
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [DiagnosticCode::TK2339]
        );
    }
}
