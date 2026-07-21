//! Strict test-only snapshot boundary for the type universe.

use super::*;
use crate::snapshot_codec::{SnapshotCodecError, SnapshotReader, SnapshotWriter};
use crate::types::hash::StructuralKey;
use crate::types::repr::{
    ClassInstanceType, DeferredIndexedAccessType, FunctionType, TemplateType,
};

const VERSION: u32 = 1;
const WELL_KNOWN_COUNT: usize = 16;

impl Interner {
    pub(crate) fn write_snapshot_for_test(
        &self,
        writer: &mut SnapshotWriter,
    ) -> Result<(), SnapshotCodecError> {
        writer.u32(VERSION);
        self.store.write_snapshot_for_test(writer)?;

        let mut buckets = self.dedup.iter().collect::<Vec<_>>();
        buckets.sort_by_key(|(hash, _)| **hash);
        writer.usize(buckets.len())?;
        for (hash, candidates) in buckets {
            writer.u64(*hash);
            let mut candidates = candidates.iter().copied().collect::<Vec<_>>();
            candidates.sort_unstable();
            if !candidates.windows(2).all(|pair| pair[0] < pair[1]) {
                return Err(validation("dedup bucket contains duplicate TypeIds"));
            }
            writer.usize(candidates.len())?;
            for candidate in candidates {
                writer.u32(candidate.0);
            }
        }

        let mut reserved = self.reserved_types.iter().collect::<Vec<_>>();
        reserved.sort_by_key(|(id, _)| id.0);
        writer.usize(reserved.len())?;
        for (id, terminal) in reserved {
            if terminal.state != ReservedTypeState::Frozen {
                return Err(validation("snapshot cannot expose a pending reserved type"));
            }
            writer.u32(id.0);
            writer.u8(reserved_kind_discriminant(terminal.kind));
        }

        for id in well_known_ids(self.well_known) {
            writer.u32(id.0);
        }
        Ok(())
    }

    pub(crate) fn read_snapshot_for_test(
        reader: &mut SnapshotReader<'_>,
    ) -> Result<Self, SnapshotCodecError> {
        let version_offset = reader.position();
        if reader.u32()? != VERSION {
            return Err(SnapshotCodecError::invalid(
                version_offset,
                "unsupported interner snapshot version",
            ));
        }
        let store = Store::read_snapshot_for_test(reader)?;

        let bucket_count = reader.collection_len(16)?;
        let mut dedup = FxHashMap::default();
        let mut previous_hash = None;
        for _ in 0..bucket_count {
            let hash = reader.u64()?;
            if previous_hash.is_some_and(|previous| previous >= hash) {
                return Err(SnapshotCodecError::invalid(
                    reader.position(),
                    "dedup buckets are not strictly ordered",
                ));
            }
            let candidate_count = reader.collection_len(4)?;
            if candidate_count == 0 {
                return Err(validation("dedup bucket is empty"));
            }
            let mut candidates = SmallVec::<[TypeId; 2]>::new();
            let mut previous_candidate = None;
            for _ in 0..candidate_count {
                let candidate = TypeId(reader.u32()?);
                if previous_candidate.is_some_and(|previous| previous >= candidate) {
                    return Err(SnapshotCodecError::invalid(
                        reader.position(),
                        "dedup candidates are not strictly ordered",
                    ));
                }
                candidates.push(candidate);
                previous_candidate = Some(candidate);
            }
            if dedup.insert(hash, candidates).is_some() {
                return Err(validation("duplicate dedup bucket"));
            }
            previous_hash = Some(hash);
        }

        let reserved_count = reader.collection_len(5)?;
        let mut reserved_types = FxHashMap::default();
        let mut previous_reserved = None;
        for _ in 0..reserved_count {
            let id = TypeId(reader.u32()?);
            if previous_reserved.is_some_and(|previous| previous >= id) {
                return Err(SnapshotCodecError::invalid(
                    reader.position(),
                    "reserved terminals are not strictly ordered",
                ));
            }
            let kind_offset = reader.position();
            let kind = read_reserved_kind(reader.u8()?, kind_offset)?;
            if reserved_types
                .insert(
                    id,
                    ReservedType {
                        kind,
                        state: ReservedTypeState::Frozen,
                    },
                )
                .is_some()
            {
                return Err(validation("duplicate reserved terminal"));
            }
            previous_reserved = Some(id);
        }

        let mut ids = [TypeId(0); WELL_KNOWN_COUNT];
        for id in &mut ids {
            *id = TypeId(reader.u32()?);
        }
        let well_known = well_known_from_ids(ids);

        let interner = Interner {
            store,
            dedup,
            reserved_types,
            well_known,
        };
        interner.validate_snapshot_for_test()?;
        Ok(interner)
    }

    pub(crate) fn encode_snapshot_bytes_for_test(&self) -> Result<Vec<u8>, SnapshotCodecError> {
        let mut writer = SnapshotWriter::new();
        self.write_snapshot_for_test(&mut writer)?;
        Ok(writer.into_bytes())
    }

    pub(crate) fn decode_snapshot_bytes_for_test(bytes: &[u8]) -> Result<Self, SnapshotCodecError> {
        let mut reader = SnapshotReader::new(bytes);
        let interner = Self::read_snapshot_for_test(&mut reader)?;
        reader.finish()?;
        Ok(interner)
    }

    fn validate_snapshot_for_test(&self) -> Result<(), SnapshotCodecError> {
        let len = self.store.len();
        if len < WELL_KNOWN_COUNT {
            return Err(validation("store is missing canonical intrinsic rows"));
        }

        let ids = well_known_ids(self.well_known);
        for (index, (id, kind)) in ids.into_iter().zip(IntrinsicKind::ALL).enumerate() {
            if id.index() != index || self.store.intrinsic_kind(id) != Some(kind) {
                return Err(validation("well-known intrinsic identity mismatch"));
            }
        }

        for (id, reserved) in &self.reserved_types {
            if id.index() >= len || reserved.state != ReservedTypeState::Frozen {
                return Err(validation("reserved terminal is invalid"));
            }
            let expected_tag = match reserved.kind {
                ReservedTypeKind::Object => TypeTag::Object,
                ReservedTypeKind::Conditional => TypeTag::Conditional,
                ReservedTypeKind::Mapped => TypeTag::Mapped,
            };
            if self.store.tag(*id) != expected_tag {
                return Err(validation("reserved kind does not match its store row"));
            }
        }
        for id in self.store.snapshot_template_name_ids_for_test() {
            if !self.reserved_types.contains_key(&id) {
                return Err(validation(
                    "template display name is not attached to a reservation",
                ));
            }
        }

        let mut seen = vec![false; len];
        for (hash, candidates) in &self.dedup {
            if candidates.is_empty() {
                return Err(validation("dedup bucket is empty"));
            }
            for (position, candidate) in candidates.iter().copied().enumerate() {
                if candidate.index() >= len
                    || self.reserved_types.contains_key(&candidate)
                    || std::mem::replace(&mut seen[candidate.index()], true)
                {
                    return Err(validation("dedup candidate coverage is invalid"));
                }
                if structural_hash_for_id(&self.store, candidate)? != *hash {
                    return Err(validation("dedup hash does not match its candidate"));
                }
                for previous in &candidates[..position] {
                    if structurally_equal(&self.store, *previous, candidate)? {
                        return Err(validation("dedup bucket contains duplicate identities"));
                    }
                }
            }
        }
        for (index, covered) in seen.into_iter().enumerate() {
            let id = TypeId(
                u32::try_from(index)
                    .map_err(|_| validation("store length exceeds the TypeId range"))?,
            );
            if covered == self.reserved_types.contains_key(&id) {
                return Err(validation("dedup does not exactly cover structural rows"));
            }
        }

        for index in 0..len {
            let id = TypeId(
                u32::try_from(index)
                    .map_err(|_| validation("store length exceeds the TypeId range"))?,
            );
            let Some(members) = self.store.union_members(id) else {
                continue;
            };
            if members.iter().any(|member| {
                *member == self.well_known.never
                    || *member == self.well_known.any
                    || *member == self.well_known.error
                    || *member == self.well_known.unknown
            }) {
                return Err(validation("union contains an absorbed or identity member"));
            }
        }
        for index in 0..len {
            let id = TypeId(
                u32::try_from(index)
                    .map_err(|_| validation("store length exceeds the TypeId range"))?,
            );
            let Some(members) = self.store.intersection_members(id) else {
                continue;
            };
            if members.iter().any(|member| {
                *member == self.well_known.unknown
                    || *member == self.well_known.any
                    || *member == self.well_known.error
                    || *member == self.well_known.never
            }) {
                return Err(validation(
                    "intersection contains an absorbed or identity member",
                ));
            }
        }
        Ok(())
    }
}

fn validation(message: &'static str) -> SnapshotCodecError {
    SnapshotCodecError::invalid(0, message)
}

fn well_known_ids(well_known: WellKnown) -> [TypeId; WELL_KNOWN_COUNT] {
    [
        well_known.error,
        well_known.any,
        well_known.unknown,
        well_known.never,
        well_known.void,
        well_known.null,
        well_known.undefined,
        well_known.boolean,
        well_known.number,
        well_known.string,
        well_known.uppercase,
        well_known.lowercase,
        well_known.capitalize,
        well_known.uncapitalize,
        well_known.this_type,
        well_known.omit_this_parameter,
    ]
}

fn well_known_from_ids(ids: [TypeId; WELL_KNOWN_COUNT]) -> WellKnown {
    let [error, any, unknown, never, void, null, undefined, boolean, number, string, uppercase, lowercase, capitalize, uncapitalize, this_type, omit_this_parameter] =
        ids;
    WellKnown {
        error,
        any,
        unknown,
        never,
        void,
        null,
        undefined,
        boolean,
        number,
        string,
        uppercase,
        lowercase,
        capitalize,
        uncapitalize,
        this_type,
        omit_this_parameter,
    }
}

fn reserved_kind_discriminant(kind: ReservedTypeKind) -> u8 {
    match kind {
        ReservedTypeKind::Object => 0,
        ReservedTypeKind::Conditional => 1,
        ReservedTypeKind::Mapped => 2,
    }
}

fn read_reserved_kind(value: u8, offset: usize) -> Result<ReservedTypeKind, SnapshotCodecError> {
    match value {
        0 => Ok(ReservedTypeKind::Object),
        1 => Ok(ReservedTypeKind::Conditional),
        2 => Ok(ReservedTypeKind::Mapped),
        _ => Err(SnapshotCodecError::invalid(
            offset,
            "invalid reserved kind discriminant",
        )),
    }
}

fn structural_hash_for_id(store: &Store, id: TypeId) -> Result<u64, SnapshotCodecError> {
    let hash = match store.tag(id) {
        TypeTag::Intrinsic => structural_hash(&StructuralKey::Intrinsic(
            store
                .intrinsic_kind(id)
                .ok_or_else(|| validation("intrinsic payload is missing"))?,
        )),
        TypeTag::Literal => structural_hash(&StructuralKey::Literal(
            store
                .literal_value(id)
                .ok_or_else(|| validation("literal payload is missing"))?,
        )),
        TypeTag::Object => {
            let object = store
                .object_type(id)
                .ok_or_else(|| validation("object payload is missing"))?;
            structural_hash(&StructuralKey::Object {
                properties: &object.properties,
                string_index: object.string_index,
                number_index: object.number_index,
                call_signatures: &object.call_signatures,
                construct_signatures: &object.construct_signatures,
            })
        }
        TypeTag::Union => structural_hash(&StructuralKey::Union(
            store
                .union_members(id)
                .ok_or_else(|| validation("union payload is missing"))?,
        )),
        TypeTag::Intersection => structural_hash(&StructuralKey::Intersection(
            store
                .intersection_members(id)
                .ok_or_else(|| validation("intersection payload is missing"))?,
        )),
        TypeTag::Function => {
            let function = store
                .function_type(id)
                .ok_or_else(|| validation("function payload is missing"))?;
            structural_hash(&StructuralKey::Function {
                type_params: &function.type_params,
                receiver: function.receiver,
                params: &function.params,
                ret: function.ret,
            })
        }
        TypeTag::TypeParam => structural_hash(&StructuralKey::TypeParam(
            store
                .type_param(id)
                .ok_or_else(|| validation("type parameter payload is missing"))?
                .id,
        )),
        TypeTag::Array => structural_hash(&StructuralKey::Array(
            store
                .array_type(id)
                .ok_or_else(|| validation("array payload is missing"))?
                .element,
        )),
        TypeTag::Tuple => structural_hash(&StructuralKey::Tuple(
            store
                .tuple_type(id)
                .ok_or_else(|| validation("tuple payload is missing"))?,
        )),
        TypeTag::Readonly => structural_hash(&StructuralKey::Readonly(
            store
                .readonly_operand(id)
                .ok_or_else(|| validation("readonly payload is missing"))?,
        )),
        TypeTag::Conditional => {
            let conditional = store
                .conditional_type(id)
                .ok_or_else(|| validation("conditional payload is missing"))?;
            structural_hash(&StructuralKey::Conditional {
                check: conditional.check,
                extends_ty: conditional.extends_ty,
                true_branch: conditional.true_branch,
                false_branch: conditional.false_branch,
                infer_count: conditional.infer_count,
                distributive: conditional.distributive,
                poisoned: conditional.poisoned,
            })
        }
        TypeTag::Instantiation => {
            let instantiation = store
                .instantiation_type(id)
                .ok_or_else(|| validation("instantiation payload is missing"))?;
            structural_hash(&StructuralKey::Instantiation {
                base: instantiation.base,
                args: &instantiation.args,
            })
        }
        TypeTag::Infer => structural_hash(&StructuralKey::Infer(
            store
                .infer_index(id)
                .ok_or_else(|| validation("infer payload is missing"))?,
        )),
        TypeTag::Mapped => {
            let mapped = store
                .mapped_type(id)
                .ok_or_else(|| validation("mapped payload is missing"))?;
            structural_hash(&StructuralKey::Mapped {
                homomorphic: mapped.homomorphic,
                key_source: mapped.key_source,
                value_template: mapped.value_template,
                modifiers_source: mapped.modifiers_source,
                optional_modifier: mapped.optional_modifier,
                readonly_modifier: mapped.readonly_modifier,
            })
        }
        TypeTag::MappedValue => structural_hash(&StructuralKey::MappedValue),
        TypeTag::Template => {
            let template = store
                .template_type(id)
                .ok_or_else(|| validation("template payload is missing"))?;
            structural_hash(&StructuralKey::Template {
                texts: &template.texts,
                holes: &template.holes,
            })
        }
        TypeTag::Keyof => structural_hash(&StructuralKey::Keyof(
            store
                .keyof_operand(id)
                .ok_or_else(|| validation("keyof payload is missing"))?,
        )),
        TypeTag::ClassInstance => {
            let instance = store
                .class_instance_type(id)
                .ok_or_else(|| validation("class instance payload is missing"))?;
            structural_hash(&StructuralKey::ClassInstance {
                class: instance.class,
                args: &instance.args,
            })
        }
        TypeTag::DeferredIndexedAccess => {
            let access = store
                .deferred_indexed_access_type(id)
                .ok_or_else(|| validation("indexed access payload is missing"))?;
            structural_hash(&StructuralKey::DeferredIndexedAccess {
                object: access.object,
                index: access.index,
            })
        }
    };
    Ok(hash)
}

fn structurally_equal(
    store: &Store,
    left: TypeId,
    right: TypeId,
) -> Result<bool, SnapshotCodecError> {
    if store.tag(left) != store.tag(right) {
        return Ok(false);
    }
    let equal = match store.tag(left) {
        TypeTag::Intrinsic => store.intrinsic_kind(left) == store.intrinsic_kind(right),
        TypeTag::Literal => store.literal_value(left) == store.literal_value(right),
        TypeTag::Object => {
            let left = object(store, left)?;
            let right = object(store, right)?;
            left.string_index == right.string_index
                && left.number_index == right.number_index
                && left.call_signatures == right.call_signatures
                && left.construct_signatures == right.construct_signatures
                && object_props_eq(&left.properties, &right.properties)
        }
        TypeTag::Union => store.union_members(left) == store.union_members(right),
        TypeTag::Intersection => {
            store.intersection_members(left) == store.intersection_members(right)
        }
        TypeTag::Function => function_equal(function(store, left)?, function(store, right)?),
        TypeTag::TypeParam => {
            store.type_param(left).map(|parameter| parameter.id)
                == store.type_param(right).map(|parameter| parameter.id)
        }
        TypeTag::Array => {
            store.array_type(left).map(|array| array.element)
                == store.array_type(right).map(|array| array.element)
        }
        TypeTag::Tuple => store.tuple_type(left) == store.tuple_type(right),
        TypeTag::Readonly => store.readonly_operand(left) == store.readonly_operand(right),
        TypeTag::Conditional => conditional_equal(store, left, right)?,
        TypeTag::Instantiation => instantiation_equal(store, left, right)?,
        TypeTag::Infer => store.infer_index(left) == store.infer_index(right),
        TypeTag::Mapped => mapped_equal(store, left, right)?,
        TypeTag::MappedValue => true,
        TypeTag::Template => template_equal(store, left, right)?,
        TypeTag::Keyof => store.keyof_operand(left) == store.keyof_operand(right),
        TypeTag::ClassInstance => class_instance(store, left)? == class_instance(store, right)?,
        TypeTag::DeferredIndexedAccess => {
            deferred_access(store, left)? == deferred_access(store, right)?
        }
    };
    Ok(equal)
}

fn object(store: &Store, id: TypeId) -> Result<&ObjectType, SnapshotCodecError> {
    store
        .object_type(id)
        .ok_or_else(|| validation("object payload is missing"))
}

fn function(store: &Store, id: TypeId) -> Result<&FunctionType, SnapshotCodecError> {
    store
        .function_type(id)
        .ok_or_else(|| validation("function payload is missing"))
}

fn function_equal(left: &FunctionType, right: &FunctionType) -> bool {
    left.type_params == right.type_params
        && left.receiver == right.receiver
        && left.params == right.params
        && left.ret == right.ret
}

fn conditional_equal(
    store: &Store,
    left: TypeId,
    right: TypeId,
) -> Result<bool, SnapshotCodecError> {
    let left = store
        .conditional_type(left)
        .ok_or_else(|| validation("conditional payload is missing"))?;
    let right = store
        .conditional_type(right)
        .ok_or_else(|| validation("conditional payload is missing"))?;
    Ok(left.check == right.check
        && left.extends_ty == right.extends_ty
        && left.true_branch == right.true_branch
        && left.false_branch == right.false_branch
        && left.infer_count == right.infer_count
        && left.distributive == right.distributive
        && left.poisoned == right.poisoned)
}

fn instantiation_equal(
    store: &Store,
    left: TypeId,
    right: TypeId,
) -> Result<bool, SnapshotCodecError> {
    let left = store
        .instantiation_type(left)
        .ok_or_else(|| validation("instantiation payload is missing"))?;
    let right = store
        .instantiation_type(right)
        .ok_or_else(|| validation("instantiation payload is missing"))?;
    Ok(left.base == right.base && left.args == right.args)
}

fn mapped_equal(store: &Store, left: TypeId, right: TypeId) -> Result<bool, SnapshotCodecError> {
    let left = store
        .mapped_type(left)
        .ok_or_else(|| validation("mapped payload is missing"))?;
    let right = store
        .mapped_type(right)
        .ok_or_else(|| validation("mapped payload is missing"))?;
    Ok(left.homomorphic == right.homomorphic
        && left.key_source == right.key_source
        && left.value_template == right.value_template
        && left.modifiers_source == right.modifiers_source
        && left.optional_modifier == right.optional_modifier
        && left.readonly_modifier == right.readonly_modifier)
}

fn template_equal(store: &Store, left: TypeId, right: TypeId) -> Result<bool, SnapshotCodecError> {
    let left = template(store, left)?;
    let right = template(store, right)?;
    Ok(left.texts == right.texts && left.holes == right.holes)
}

fn template(store: &Store, id: TypeId) -> Result<&TemplateType, SnapshotCodecError> {
    store
        .template_type(id)
        .ok_or_else(|| validation("template payload is missing"))
}

fn class_instance(store: &Store, id: TypeId) -> Result<&ClassInstanceType, SnapshotCodecError> {
    store
        .class_instance_type(id)
        .ok_or_else(|| validation("class instance payload is missing"))
}

fn deferred_access(
    store: &Store,
    id: TypeId,
) -> Result<&DeferredIndexedAccessType, SnapshotCodecError> {
    store
        .deferred_indexed_access_type(id)
        .ok_or_else(|| validation("indexed access payload is missing"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{
        ConditionalType, FunctionType, MappedType, ModifierOp, ObjectType, ParameterType,
        PropertyType, TemplateType, TupleRestType, TupleType,
    };

    fn rich_interner() -> Interner {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let literal = interner.intern_literal(LiteralValue::String("snapshot".to_owned()));
        let _number_literal = interner.intern_literal(LiteralValue::Number(-0.0));
        let _boolean_literal = interner.intern_literal(LiteralValue::Boolean(true));
        let parameter_id = TypeParamId(7);
        let parameter = interner.intern_type_param(parameter_id, "T");
        assert!(interner.set_type_param_constraint(parameter_id, wk.string));
        interner
            .freeze_type_param_metadata(&[parameter_id])
            .expect("fresh parameter freezes");
        let array = interner.intern_array(parameter);
        let _readonly = interner.intern_readonly(array);
        let tuple = interner.intern_tuple_type(TupleType::with_rest(
            vec![literal],
            TupleRestType::new(1, array),
        ));
        let function = interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params: vec![ParameterType::required("value", tuple)],
            ret: wk.boolean,
        });
        let object = interner.reserve_object();
        interner.fill_object(
            object,
            ObjectType {
                properties: vec![PropertyType::public("next", object)],
                string_index: Some(literal),
                number_index: None,
                call_signatures: vec![function],
                construct_signatures: Vec::new(),
            },
        );
        let conditional = interner.reserve_conditional();
        interner.fill_conditional(
            conditional,
            ConditionalType {
                check: parameter,
                extends_ty: wk.string,
                true_branch: object,
                false_branch: wk.never,
                infer_count: 0,
                distributive: true,
                poisoned: false,
            },
        );
        interner.set_template_name(conditional, "SnapshotConditional");
        let _instantiation =
            interner.intern_instantiation(conditional, vec![(parameter_id, literal)]);
        let _infer = interner.intern_infer(0);
        let mapped_value = interner.intern_mapped_value();
        let mapped = interner.reserve_mapped();
        interner.fill_mapped(
            mapped,
            MappedType {
                homomorphic: true,
                key_source: wk.string,
                value_template: mapped_value,
                modifiers_source: Some(object),
                optional_modifier: ModifierOp::Add,
                readonly_modifier: ModifierOp::Remove,
            },
        );
        interner.set_template_name(mapped, "SnapshotMapped");
        let _template = interner.intern_template(TemplateType {
            texts: vec!["before".to_owned(), "after".to_owned()],
            holes: vec![literal],
        });
        let _union = interner.union(vec![literal, wk.string]);
        let _intersection = interner.intersection(vec![object, mapped]);
        let _keyof = interner.intern_keyof(parameter);
        let class_instance =
            interner.intern_class_instance(crate::types::repr::ClassId(11), vec![literal]);
        let _indexed = interner.intern_deferred_indexed_access(class_instance, parameter);
        interner
    }

    #[test]
    fn type_snapshot_roundtrip_preserves_exact_bytes_and_suffix_identity() {
        let interner = rich_interner();
        let bytes = interner
            .encode_snapshot_bytes_for_test()
            .expect("rich type universe encodes");
        let mut decoded =
            Interner::decode_snapshot_bytes_for_test(&bytes).expect("rich type universe decodes");
        assert_eq!(
            decoded
                .encode_snapshot_bytes_for_test()
                .expect("decoded universe re-encodes"),
            bytes
        );
        assert_eq!(decoded.store().len(), interner.store().len());
        let prior = decoded.store().len();
        let fresh = decoded.intern_literal(LiteralValue::String("suffix".to_owned()));
        assert_eq!(fresh.index(), prior);
    }

    #[test]
    fn type_snapshot_rejects_truncation_trailing_bytes_and_bad_discriminants() {
        let interner = rich_interner();
        let bytes = interner
            .encode_snapshot_bytes_for_test()
            .expect("rich type universe encodes");
        assert!(Interner::decode_snapshot_bytes_for_test(&bytes[..bytes.len() - 1]).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(Interner::decode_snapshot_bytes_for_test(&trailing).is_err());

        // Interner version (4 bytes), Store version (4), row count (8), first tag.
        let mut bad_tag = bytes;
        bad_tag[16] = u8::MAX;
        assert!(Interner::decode_snapshot_bytes_for_test(&bad_tag).is_err());

        let mut bad_stable_hash = interner
            .encode_snapshot_bytes_for_test()
            .expect("rich type universe encodes");
        // Interner version, Store version/count, then tag/flags/payload.
        bad_stable_hash[25] = 1;
        assert!(Interner::decode_snapshot_bytes_for_test(&bad_stable_hash).is_err());

        let mut bad_well_known = interner
            .encode_snapshot_bytes_for_test()
            .expect("rich type universe encodes");
        let well_known_start = bad_well_known.len() - WELL_KNOWN_COUNT * 4;
        bad_well_known[well_known_start..well_known_start + 4]
            .copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(Interner::decode_snapshot_bytes_for_test(&bad_well_known).is_err());
    }
}
