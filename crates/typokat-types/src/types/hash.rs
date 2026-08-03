//! Structural hashing for the interner.
//!
//! `structural_hash` is the live FxHash key; `StableHash` reserves the future
//! cross-run content hash slot and deliberately returns a zero digest today.

use crate::types::repr::{
    ClassId, DeclaredRecipeId, GenericTypeParam, IntrinsicKind, LiteralValue, ModifierOp,
    ParameterType, PropertyType, TupleType, TypeParamId, TypeTag,
};
use crate::types::store::TypeId;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

/// The pre-intern structural-hash key. It mirrors the data the interner dedups
/// on, so structurally equal candidates hash into the same bucket and then get
/// confirmed by structural comparison.
pub enum StructuralKey<'a> {
    Intrinsic(IntrinsicKind),
    Literal(&'a LiteralValue),
    /// Object identity: canonical property list plus index and call/construct
    /// signatures. All identity-bearing `PropertyType` fields are hashed below.
    Object {
        properties: &'a [PropertyType],
        string_index: Option<TypeId>,
        number_index: Option<TypeId>,
        call_signatures: &'a [TypeId],
        construct_signatures: &'a [TypeId],
    },
    /// A function type, keyed over its ordered generic binders, optional non-positional
    /// receiver, positional parameter list (never sorted), and return type.
    Function {
        type_params: &'a [GenericTypeParam],
        receiver: Option<TypeId>,
        params: &'a [ParameterType],
        ret: TypeId,
    },
    /// A union type, keyed over its **canonical** member list (flattened, sorted
    /// by `TypeId`, deduped, `never`-free by the interner before hashing) so two
    /// unions with the same member set in any source order collide.
    Union(&'a [TypeId]),
    /// An **intersection** type (M31), keyed over its **canonical** member list
    /// (flattened, sorted by `TypeId`, deduped, `unknown`-free by the interner
    /// before hashing). The discriminant is hashed distinctly from `Union`, so a
    /// union and an intersection over the *same* member set never collide.
    Intersection(&'a [TypeId]),
    /// A **type parameter** (M9), keyed over its [`TypeParamId`] alone. The name
    /// is **not** part of the key — identity is the declaration site's unique id,
    /// so re-interning the same parameter collides while two parameters from
    /// distinct declarations never do (even if they share a source name).
    TypeParam(TypeParamId),
    /// An **array** type (`T[]` — M17), keyed over its **element** `TypeId` alone.
    /// `number[]` collides with `number[]`; `number[]` and `string[]` do not. The
    /// element is itself canonical (interned), so the key is decided by its id.
    Array(TypeId),
    /// A **tuple** type (`[A, B]` — M18), keyed over its ordered fixed element ids
    /// plus any M32 rest segment.
    Tuple(&'a TupleType),
    /// A **readonly** wrapper over an array/tuple operand. Identity is the operand
    /// id alone, with its own discriminant so it never collides with the operand.
    Readonly(TypeId),
    /// A **conditional** type (`C extends E ? T : F` — M25), keyed over its four
    /// component ids **in order** (position is meaning — the branches are not
    /// interchangeable), plus `infer_count` and `distributive`. So
    /// `C extends E ? T : F` and `C extends E ? F : T` hash differently.
    Conditional {
        check: TypeId,
        extends_ty: TypeId,
        true_branch: TypeId,
        false_branch: TypeId,
        infer_count: u32,
        distributive: bool,
        poisoned: bool,
    },
    /// A **lazy instantiation** (M25), keyed over its base id and its
    /// **sorted-by-id** argument list. Two equal `(base, args)` collide.
    Instantiation {
        base: TypeId,
        args: &'a [(TypeParamId, TypeId)],
    },
    /// An immutable class application keyed by declaration identity plus arguments
    /// in declaration order.
    ClassInstance {
        class: ClassId,
        args: &'a [TypeId],
    },
    /// An **`infer` binder** (M25), keyed over its de Bruijn index alone.
    Infer(u32),
    /// A **mapped type** (M26), keyed over its whole shape (homomorphic flag, key
    /// source, value template, modifiers source (M28), and both modifier operators) so
    /// two structurally equal mapped types collide.
    Mapped {
        homomorphic: bool,
        key_source: TypeId,
        value_template: TypeId,
        modifiers_source: Option<TypeId>,
        optional_modifier: ModifierOp,
        readonly_modifier: ModifierOp,
    },
    /// A **mapped-value placeholder** (M26). Identity is the tag alone (no payload), so
    /// every `T[K]` placeholder hash-conses to one node.
    MappedValue,
    /// A **template literal type** (M27), keyed over its ordered text segments and hole
    /// ids (position is meaning — a template is a sequence, not a set), so two
    /// structurally equal templates collide.
    Template {
        texts: &'a [String],
        holes: &'a [TypeId],
    },
    /// A **deferred `keyof`** (M28), keyed over its operand id alone. `keyof T`
    /// collides with `keyof T`; `keyof T` and `keyof U` do not.
    Keyof(TypeId),
    /// A deferred indexed access keyed by its ordered object/index operands.
    DeferredIndexedAccess {
        object: TypeId,
        index: TypeId,
    },
    Declared {
        recipe: DeclaredRecipeId,
        mapper: &'a [(TypeParamId, TypeId)],
    },
}

/// Live structural hash used for hash-consing (FxHash for speed). This is the
/// function `Interner` keys its dedup map on.
pub fn structural_hash(key: &StructuralKey<'_>) -> u64 {
    let mut h = FxHasher::default();
    match key {
        StructuralKey::Intrinsic(kind) => {
            // Tag-discriminate so an intrinsic and a (future) literal can never
            // collide on the same numeric payload.
            TypeTag::Intrinsic.hash_discriminant(&mut h);
            (*kind as u8).hash(&mut h);
        }
        StructuralKey::Literal(value) => {
            TypeTag::Literal.hash_discriminant(&mut h);
            hash_literal(value, &mut h);
        }
        StructuralKey::Object {
            properties,
            string_index,
            number_index,
            call_signatures,
            construct_signatures,
        } => {
            TypeTag::Object.hash_discriminant(&mut h);
            // M19: fold the index signatures into the key first (the value-type id,
            // or a sentinel for absent), so an object with an index signature hashes
            // distinctly from one without. The ids are canonical (interned), so
            // hashing them by value is order-stable.
            string_index.map(|v| v.0).hash(&mut h);
            number_index.map(|v| v.0).hash(&mut h);
            // F1/WU2: call signatures are part of object identity. Hash both the
            // count and each interned FunctionType id so `{ (x: number): string }`
            // is distinct from `{}` and from a same-member object with a different
            // call signature.
            call_signatures.len().hash(&mut h);
            for signature in *call_signatures {
                signature.0.hash(&mut h);
            }
            // F1/WU3: construct signatures are the `new` dual of call
            // signatures and participate in object identity the same way.
            construct_signatures.len().hash(&mut h);
            for signature in *construct_signatures {
                signature.0.hash(&mut h);
            }
            // Length first so prefixes of a longer property list cannot collide
            // with the shorter one under the streaming hasher.
            properties.len().hash(&mut h);
            for prop in *properties {
                // Properties arrive in canonical (key-sorted) order, so this is
                // order-independent across two structurally equal object types.
                prop.key.hash(&mut h);
                prop.optional.hash(&mut h);
                prop.ty.0.hash(&mut h);
                prop.write_ty.map(|ty| ty.0).hash(&mut h);
                // Property identity fields are hashed here so non-public origins
                // keep nominal ids and relation-cache keys sound; see `PropertyType`.
                (prop.visibility as u8).hash(&mut h);
                prop.declaring_class.map(|c| c.0).hash(&mut h);
                prop.readonly.hash(&mut h);
                prop.is_accessor.hash(&mut h);
            }
        }
        StructuralKey::Function {
            type_params,
            receiver,
            params,
            ret,
        } => {
            TypeTag::Function.hash_discriminant(&mut h);
            type_params.len().hash(&mut h);
            for param in *type_params {
                param.id.0.hash(&mut h);
                param.constraint.map(|ty| ty.0).hash(&mut h);
                param.default.map(|ty| ty.0).hash(&mut h);
            }
            receiver.map(|ty| ty.0).hash(&mut h);
            // Arity first so a shorter parameter list cannot collide with a
            // prefix of a longer one under the streaming hasher.
            params.len().hash(&mut h);
            for param in *params {
                // Parameters are positional (never sorted): hash in order so two
                // function types with the same parameter *types* in a different
                // order remain distinct.
                param.name.hash(&mut h);
                param.optional.hash(&mut h);
                param.has_default.hash(&mut h);
                param.rest.hash(&mut h);
                param.ty.0.hash(&mut h);
            }
            ret.0.hash(&mut h);
        }
        StructuralKey::Union(members) => {
            TypeTag::Union.hash_discriminant(&mut h);
            // Arity first so a shorter member list cannot collide with a prefix
            // of a longer one under the streaming hasher.
            members.len().hash(&mut h);
            for member in *members {
                // Members arrive in canonical (TypeId-sorted) order, so this is
                // order-independent across two structurally equal union types.
                member.0.hash(&mut h);
            }
        }
        StructuralKey::Intersection(members) => {
            // Discriminant first (distinct from `Union`), so a union and an
            // intersection over the same member set never collide.
            TypeTag::Intersection.hash_discriminant(&mut h);
            // Arity first so a shorter member list cannot collide with a prefix
            // of a longer one under the streaming hasher.
            members.len().hash(&mut h);
            for member in *members {
                // Members arrive in canonical (TypeId-sorted) order, so this is
                // order-independent across two structurally equal intersections.
                member.0.hash(&mut h);
            }
        }
        StructuralKey::TypeParam(id) => {
            TypeTag::TypeParam.hash_discriminant(&mut h);
            // Identity is the declaration-site id only (not the name).
            id.0.hash(&mut h);
        }
        StructuralKey::Array(element) => {
            TypeTag::Array.hash_discriminant(&mut h);
            // Identity is the (canonical) element id alone.
            element.0.hash(&mut h);
        }
        StructuralKey::Tuple(tuple) => {
            TypeTag::Tuple.hash_discriminant(&mut h);
            // Length first so a shorter element list cannot collide with a prefix
            // of a longer one under the streaming hasher (and so `[number]` differs
            // from `[number, string]`).
            tuple.elements.len().hash(&mut h);
            for element in &tuple.elements {
                // Elements are hashed in **source order** (never sorted), so order
                // is part of identity: `[number, string]` and `[string, number]`
                // hash differently.
                element.0.hash(&mut h);
            }
            match tuple.rest {
                Some(rest) => {
                    true.hash(&mut h);
                    rest.position.hash(&mut h);
                    rest.ty.0.hash(&mut h);
                }
                None => false.hash(&mut h),
            }
        }
        StructuralKey::Readonly(operand) => {
            TypeTag::Readonly.hash_discriminant(&mut h);
            operand.0.hash(&mut h);
        }
        StructuralKey::Conditional {
            check,
            extends_ty,
            true_branch,
            false_branch,
            infer_count,
            distributive,
            poisoned,
        } => {
            TypeTag::Conditional.hash_discriminant(&mut h);
            // Hashed in field order (position is meaning — the branches are not a set).
            check.0.hash(&mut h);
            extends_ty.0.hash(&mut h);
            true_branch.0.hash(&mut h);
            false_branch.0.hash(&mut h);
            infer_count.hash(&mut h);
            distributive.hash(&mut h);
            poisoned.hash(&mut h);
        }
        StructuralKey::Instantiation { base, args } => {
            TypeTag::Instantiation.hash_discriminant(&mut h);
            base.0.hash(&mut h);
            // Args arrive sorted by TypeParamId, so this is order-stable across two
            // structurally equal instantiations.
            args.len().hash(&mut h);
            for (param, arg) in *args {
                param.0.hash(&mut h);
                arg.0.hash(&mut h);
            }
        }
        StructuralKey::ClassInstance { class, args } => {
            TypeTag::ClassInstance.hash_discriminant(&mut h);
            class.0.hash(&mut h);
            args.len().hash(&mut h);
            for arg in *args {
                arg.0.hash(&mut h);
            }
        }
        StructuralKey::Infer(index) => {
            TypeTag::Infer.hash_discriminant(&mut h);
            index.hash(&mut h);
        }
        StructuralKey::Mapped {
            homomorphic,
            key_source,
            value_template,
            modifiers_source,
            optional_modifier,
            readonly_modifier,
        } => {
            TypeTag::Mapped.hash_discriminant(&mut h);
            homomorphic.hash(&mut h);
            key_source.0.hash(&mut h);
            value_template.0.hash(&mut h);
            // M28: the modifiers source is identity-bearing — two maps differing only
            // here (a captured `T[P]` object vs none) must not collide.
            modifiers_source.map(|v| v.0).hash(&mut h);
            (*optional_modifier as u8).hash(&mut h);
            (*readonly_modifier as u8).hash(&mut h);
        }
        StructuralKey::MappedValue => {
            TypeTag::MappedValue.hash_discriminant(&mut h);
        }
        StructuralKey::Keyof(operand) => {
            TypeTag::Keyof.hash_discriminant(&mut h);
            // Identity is the (canonical) operand id alone.
            operand.0.hash(&mut h);
        }
        StructuralKey::DeferredIndexedAccess { object, index } => {
            TypeTag::DeferredIndexedAccess.hash_discriminant(&mut h);
            object.0.hash(&mut h);
            index.0.hash(&mut h);
        }
        StructuralKey::Declared { recipe, mapper } => {
            TypeTag::Declared.hash_discriminant(&mut h);
            recipe.0.hash(&mut h);
            mapper.len().hash(&mut h);
            for (parameter, argument) in *mapper {
                parameter.0.hash(&mut h);
                argument.0.hash(&mut h);
            }
        }
        StructuralKey::Template { texts, holes } => {
            TypeTag::Template.hash_discriminant(&mut h);
            // Lengths first so a shorter template cannot collide with a prefix of a
            // longer one under the streaming hasher; then the segments and holes in
            // order (a template is a sequence — position is part of identity).
            texts.len().hash(&mut h);
            for text in *texts {
                text.hash(&mut h);
            }
            holes.len().hash(&mut h);
            for hole in *holes {
                hole.0.hash(&mut h);
            }
        }
    }
    h.finish()
}

/// Hash a literal deterministically. Floats are hashed by their bit pattern so
/// `NaN` and `-0.0`/`0.0` behave consistently between the hash and the
/// `LiteralValue` `PartialEq` used for the dedup-bucket tie-break.
fn hash_literal(value: &LiteralValue, h: &mut impl Hasher) {
    match value {
        LiteralValue::Number(n) => {
            0u8.hash(h);
            n.to_bits().hash(h);
        }
        LiteralValue::String(s) => {
            1u8.hash(h);
            s.hash(h);
        }
        LiteralValue::Boolean(b) => {
            2u8.hash(h);
            b.hash(h);
        }
    }
}

impl TypeTag {
    #[inline]
    fn hash_discriminant(self, h: &mut impl Hasher) {
        (self as u8).hash(h);
    }
}

/// Reserved cross-run stable hash type.
/// TODO(Phase 4): compute a real content hash at intern time.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct StableHash(pub [u8; 32]);

/// Placeholder stable hash. NOT a real content hash — see the module docs and
/// mvp-plan §7.1. Returns the zero digest; the `Store` column is reserved but
/// unread in the MVP. Kept as a function (not `todo!()`) so it is never a
/// reachable panic.
#[allow(dead_code)] // TODO(Phase 4): wire into the interner at intern time.
pub fn stable_hash(_key: &StructuralKey<'_>) -> StableHash {
    StableHash::default()
}
