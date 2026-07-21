//! Stable full-library identities used by native-syntax member projection.

use super::context::Pass;
use super::type_groups::{
    PublishedTypeEnvironment, PublishedTypeGroupSurface, PublishedTypeGroupTerminal,
};
use crate::binder::declaration::TypeGroupId;
use crate::binder::scope::ScopeId;
use crate::binder::Binder;
use crate::span::Span;
use crate::types::repr::{
    ClassId, IntrinsicKind, LiteralValue, TypeFlags, TypeParamId, TypeTag, Visibility,
};
use crate::types::store::{Store, TypeId};
use rustc_hash::FxHashMap;

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
    array: LibraryIdentityTerminal,
    readonly_array: LibraryIdentityTerminal,
    string: LibraryIdentityTerminal,
    number: LibraryIdentityTerminal,
    boolean: LibraryIdentityTerminal,
    regexp: LibraryIdentityTerminal,
    object: LibraryIdentityTerminal,
    callable_function: LibraryIdentityTerminal,
}

#[cfg(test)]
pub(crate) type LibrarySemanticIdentitiesSnapshotParts = [LibraryIdentityTerminal; 8];

impl LibrarySemanticIdentities {
    /// Select roots once from the library compilation-global scope. Consumer scopes
    /// never participate, so same-named user declarations cannot hijack native syntax.
    #[cfg(test)]
    pub(in crate::check::checker) fn select(
        binder: &Binder,
        published: &PublishedTypeEnvironment,
        store: &Store,
    ) -> Self {
        Self::select_from_scope(binder, binder.compilation_global, published, store)
    }

    pub(in crate::check::checker) fn select_from_scope(
        binder: &Binder,
        scope: ScopeId,
        published: &PublishedTypeEnvironment,
        store: &Store,
    ) -> Self {
        Self {
            array: select_in_scope(binder, scope, published, store, "Array", 1),
            readonly_array: select_in_scope(binder, scope, published, store, "ReadonlyArray", 1),
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
        }
    }

    pub(crate) fn all_ready(&self) -> bool {
        self.terminals()
            .into_iter()
            .all(|terminal| matches!(terminal, LibraryIdentityTerminal::Ready(_)))
    }

    pub(crate) fn terminals(&self) -> [&LibraryIdentityTerminal; 8] {
        [
            &self.array,
            &self.readonly_array,
            &self.string,
            &self.number,
            &self.boolean,
            &self.regexp,
            &self.object,
            &self.callable_function,
        ]
    }

    pub(in crate::check::checker) fn array_group(&self) -> Option<TypeGroupId> {
        match &self.array {
            LibraryIdentityTerminal::Ready(identity) => Some(identity.group),
            LibraryIdentityTerminal::Unavailable(_) => None,
        }
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn callable_function_group(&self) -> Option<TypeGroupId> {
        match &self.callable_function {
            LibraryIdentityTerminal::Ready(identity) => Some(identity.group),
            LibraryIdentityTerminal::Unavailable(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_explicit_for_test(terminals: [LibraryIdentityTerminal; 8]) -> Self {
        let [array, readonly_array, string, number, boolean, regexp, object, callable_function] =
            terminals;
        Self {
            array,
            readonly_array,
            string,
            number,
            boolean,
            regexp,
            object,
            callable_function,
        }
    }

    #[cfg(test)]
    pub(crate) fn snapshot_parts(&self) -> LibrarySemanticIdentitiesSnapshotParts {
        [
            self.array.clone(),
            self.readonly_array.clone(),
            self.string.clone(),
            self.number.clone(),
            self.boolean.clone(),
            self.regexp.clone(),
            self.object.clone(),
            self.callable_function.clone(),
        ]
    }

    #[cfg(test)]
    pub(crate) fn from_snapshot_parts(
        terminals: LibrarySemanticIdentitiesSnapshotParts,
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
                return Err("snapshot semantic identity repeats a parameter id");
            }
        }
        Ok(Self::from_explicit_for_test(terminals))
    }
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

    pub(in crate::check::checker) fn lookup_library_composed_member(
        &mut self,
        ty: TypeId,
        property: &str,
        span: Span,
    ) -> LibraryComposedMember {
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
        match &identities.regexp {
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
    let readonly = interner.store().readonly_operand(ty);
    let native = readonly.unwrap_or(ty);
    if let Some(element) = interner
        .store()
        .array_type(native)
        .map(|array| array.element)
    {
        return Some(NativeMemberBridge::Generic {
            terminal: if readonly.is_some() {
                identities.readonly_array.clone()
            } else {
                identities.array.clone()
            },
            argument: element,
            incomplete_id: "library-bridge/array/identity",
        });
    }
    if let Some(tuple) = interner.store().tuple_type(native).cloned() {
        let mut elements = tuple.elements;
        if let Some(rest) = tuple.rest {
            let rest_element = interner
                .store()
                .array_type(rest.ty)
                .map(|array| array.element)
                .unwrap_or(rest.ty);
            elements.push(rest_element);
        }
        let argument = interner.union(elements);
        return Some(NativeMemberBridge::Generic {
            terminal: if readonly.is_some() {
                identities.readonly_array.clone()
            } else {
                identities.array.clone()
            },
            argument,
            incomplete_id: "library-bridge/tuple/identity",
        });
    }
    let terminal = match interner
        .store()
        .literal_value(ty)
        .map(LiteralValue::base_kind)
    {
        Some(IntrinsicKind::String) => Some((&identities.string, "library-bridge/string/identity")),
        Some(IntrinsicKind::Number) => Some((&identities.number, "library-bridge/number/identity")),
        Some(IntrinsicKind::Boolean) => {
            Some((&identities.boolean, "library-bridge/boolean/identity"))
        }
        _ => match interner.store().intrinsic_kind(ty) {
            Some(IntrinsicKind::String) => {
                Some((&identities.string, "library-bridge/string/identity"))
            }
            Some(IntrinsicKind::Number) => {
                Some((&identities.number, "library-bridge/number/identity"))
            }
            Some(IntrinsicKind::Boolean) => {
                Some((&identities.boolean, "library-bridge/boolean/identity"))
            }
            Some(IntrinsicKind::Object) => {
                Some((&identities.object, "library-bridge/object/identity"))
            }
            // FunctionType carries call signatures only; construct shapes need a
            // separate NewableFunction identity when the representation gains them.
            _ if interner.store().tag(ty) == TypeTag::Function => Some((
                &identities.callable_function,
                "library-bridge/callable-function/identity",
            )),
            _ => None,
        },
    }?;
    Some(NativeMemberBridge::Plain {
        terminal: terminal.0.clone(),
        incomplete_id: terminal.1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::bind_module_with_prelude;
    use crate::check::checker::{build_pass, context::DeclTypes};
    use crate::diagnostics::DiagnosticCode;
    use crate::driver::check_source;
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
            let expected_regexp = match &identities.regexp {
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
