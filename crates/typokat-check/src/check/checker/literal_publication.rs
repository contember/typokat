//! Shallow literal-mode finalization at checker publication boundaries.

use crate::types::repr::IntrinsicKind;
use crate::types::store::{Store, TypeId};
use crate::types::Interner;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LiteralPublicationRole {
    MutableMember,
    InferredReturn,
    ModeRetaining,
    ConstOwned,
}

pub(super) fn finalize_literal_type(
    interner: &mut Interner,
    source: TypeId,
    context: Option<TypeId>,
    role: LiteralPublicationRole,
) -> TypeId {
    match role {
        LiteralPublicationRole::ModeRetaining => source,
        LiteralPublicationRole::ConstOwned => interner.regularize_literal_type(source),
        LiteralPublicationRole::InferredReturn
            if interner.store().union_members(source).is_some() =>
        {
            source
        }
        LiteralPublicationRole::MutableMember | LiteralPublicationRole::InferredReturn => {
            let matches = context
                .is_some_and(|context| source_matches_context(interner.store(), source, context));
            let source = if matches {
                source
            } else {
                interner.widen_literal_type(source)
            };
            interner.regularize_literal_type(source)
        }
    }
}

fn source_matches_context(store: &Store, source: TypeId, context: TypeId) -> bool {
    let sources = store
        .union_members(source)
        .unwrap_or(std::slice::from_ref(&source));
    let contexts = store
        .union_members(context)
        .unwrap_or(std::slice::from_ref(&context));
    sources.iter().any(|source| {
        let Some(kind) = literal_kind(store, *source) else {
            return false;
        };
        if kind != IntrinsicKind::Boolean
            && contexts
                .iter()
                .any(|target| store.intrinsic_kind(*target) == Some(kind))
        {
            return false;
        }
        contexts.iter().any(|target| {
            literal_kind(store, *target) == Some(kind)
                || store.enum_type(*target).is_some_and(|enumeration| {
                    enumeration
                        .members
                        .iter()
                        .any(|member| member.value.base_kind() == kind)
                })
                || (kind == IntrinsicKind::Boolean
                    && store.intrinsic_kind(*target) == Some(IntrinsicKind::Boolean))
        })
    })
}

fn literal_kind(store: &Store, ty: TypeId) -> Option<IntrinsicKind> {
    store
        .literal_value(ty)
        .map(|literal| literal.base_kind())
        .or_else(|| {
            store
                .enum_member_type(ty)
                .map(|member| member.value.base_kind())
        })
}
