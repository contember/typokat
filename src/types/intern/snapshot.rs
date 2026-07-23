//! Strict snapshot boundary for the type universe, with test-only encoder support.

use super::*;
#[cfg(test)]
use crate::snapshot_codec::SnapshotWriter;
use crate::snapshot_codec::{SnapshotCodecError, SnapshotReader};
use crate::types::hash::StructuralKey;
use crate::types::repr::{
    ClassInstanceType, DeferredIndexedAccessType, FunctionType, TemplateType,
};

const VERSION: u32 = 1;
const WELL_KNOWN_COUNT: usize = 17;

// Reference-manifest domains are shared with the archive assembler.
const CONTAINER_DOMAIN: u8 = 0;
const TYPE_DOMAIN: u8 = 1;
const TYPE_PARAM_DOMAIN: u8 = 2;
const CLASS_DOMAIN: u8 = 3;
const INTERNER_BUCKET_DOMAIN: u8 = 16;

// Store TypeId owners reuse these relationship fields across payload kinds.
const TYPE_OPERAND_FIELD: u8 = 0;
const TYPE_PARAM_IDENTITY_FIELD: u8 = 1;
const CLASS_IDENTITY_FIELD: u8 = 2;
const CONSTRAINT_FIELD: u8 = 3;
const DEFAULT_FIELD: u8 = 4;
const DECLARING_CLASS_FIELD: u8 = 5;

// Store metadata container fields.
const CONSTRAINT_OWNER_FIELD: u8 = 6;
const CONSTRAINT_TARGET_FIELD: u8 = 7;
const FROZEN_TYPE_PARAM_FIELD: u8 = 8;
const TEMPLATE_NAME_TYPE_FIELD: u8 = 9;

// Interner identity container fields.
const BUCKET_CANDIDATE_FIELD: u8 = 0;
const RESERVED_TYPE_FIELD: u8 = 1;
const WELL_KNOWN_TYPE_FIELD: u8 = 2;

pub(crate) type SnapshotReferenceRecord = (u8, u8, u8, u32, u32);

fn reference(
    owner_domain: u8,
    target_domain: u8,
    field: u8,
    owner: u32,
    target: u32,
) -> SnapshotReferenceRecord {
    (owner_domain, target_domain, field, owner, target)
}

fn push_type_operand(references: &mut Vec<SnapshotReferenceRecord>, owner: TypeId, target: TypeId) {
    references.push(reference(
        TYPE_DOMAIN,
        TYPE_DOMAIN,
        TYPE_OPERAND_FIELD,
        owner.0,
        target.0,
    ));
}

fn push_type_param_identity(
    references: &mut Vec<SnapshotReferenceRecord>,
    owner: TypeId,
    target: TypeParamId,
) {
    references.push(reference(
        TYPE_DOMAIN,
        TYPE_PARAM_DOMAIN,
        TYPE_PARAM_IDENTITY_FIELD,
        owner.0,
        target.0,
    ));
}

fn push_class_identity(
    references: &mut Vec<SnapshotReferenceRecord>,
    owner: TypeId,
    field: u8,
    target: crate::types::repr::ClassId,
) {
    references.push(reference(
        TYPE_DOMAIN,
        CLASS_DOMAIN,
        field,
        owner.0,
        target.0,
    ));
}

impl Interner {
    pub(crate) fn typed_reference_records_for_replay_generation(
        &self,
    ) -> Result<Vec<SnapshotReferenceRecord>, SnapshotCodecError> {
        self.store_snapshot_reference_records()
    }

    /// Canonical reference rows for the Store and Interner archive families.
    ///
    /// Tuple order is `(owner_domain, target_domain, field, owner, target)`.
    /// The first vector is archive family 1 (Store), the second archive family
    /// 2 (Interner identity). Neither side parses the other side's wire bytes.
    pub(crate) fn snapshot_reference_records(
        &self,
    ) -> Result<(Vec<SnapshotReferenceRecord>, Vec<SnapshotReferenceRecord>), SnapshotCodecError>
    {
        if self.has_nonempty_delta() {
            return Err(validation(
                "snapshot cannot encode an interner with a non-empty delta",
            ));
        }
        self.reference_records_for_complete_state()
    }

    fn reference_records_for_complete_state(
        &self,
    ) -> Result<(Vec<SnapshotReferenceRecord>, Vec<SnapshotReferenceRecord>), SnapshotCodecError>
    {
        let store_references = self.store_snapshot_reference_records()?;
        let mut interner_references = Vec::new();

        let mut buckets = self.dedup_buckets().collect::<Vec<_>>();
        buckets.sort_unstable_by_key(|(hash, _)| **hash);
        for (bucket_index, (_, candidates)) in buckets.into_iter().enumerate() {
            let owner = u32::try_from(bucket_index)
                .map_err(|_| validation("snapshot bucket index exceeds u32"))?;
            let mut candidates = candidates.iter().copied().collect::<Vec<_>>();
            candidates.sort_unstable();
            interner_references.extend(candidates.into_iter().map(|candidate| {
                reference(
                    INTERNER_BUCKET_DOMAIN,
                    TYPE_DOMAIN,
                    BUCKET_CANDIDATE_FIELD,
                    owner,
                    candidate.0,
                )
            }));
        }

        let mut reserved = self.reserved_types().map(|(&id, _)| id).collect::<Vec<_>>();
        reserved.sort_unstable();
        for (index, id) in reserved.into_iter().enumerate() {
            interner_references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                RESERVED_TYPE_FIELD,
                u32::try_from(index)
                    .map_err(|_| validation("snapshot reserved index exceeds u32"))?,
                id.0,
            ));
        }

        for (slot, id) in well_known_ids(self.well_known).into_iter().enumerate() {
            interner_references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                WELL_KNOWN_TYPE_FIELD,
                u32::try_from(slot).map_err(|_| validation("well-known slot exceeds u32"))?,
                id.0,
            ));
        }

        interner_references.sort_unstable();
        Ok((store_references, interner_references))
    }

    #[cfg(test)]
    pub(crate) fn snapshot_reference_records_for_test(
        &self,
    ) -> (Vec<SnapshotReferenceRecord>, Vec<SnapshotReferenceRecord>) {
        self.snapshot_reference_records()
            .expect("typed interner references enumerate")
    }

    #[cfg(test)]
    pub(crate) fn local_type_reference_records_for_test(&self) -> Vec<SnapshotReferenceRecord> {
        let mut records = self
            .store_snapshot_reference_records_from(self.store.frozen_prefix_len_for_test(), true)
            .expect("typed local store references enumerate");
        for (&hash, candidates) in &self.dedup {
            let owner = u32::try_from(hash).unwrap_or(u32::MAX);
            records.extend(candidates.iter().map(|candidate| {
                reference(
                    INTERNER_BUCKET_DOMAIN,
                    TYPE_DOMAIN,
                    BUCKET_CANDIDATE_FIELD,
                    owner,
                    candidate.0,
                )
            }));
        }
        records.extend(self.reserved_types.keys().map(|id| {
            reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                RESERVED_TYPE_FIELD,
                id.0,
                id.0,
            )
        }));
        records
    }

    fn store_snapshot_reference_records(
        &self,
    ) -> Result<Vec<SnapshotReferenceRecord>, SnapshotCodecError> {
        self.store_snapshot_reference_records_from(0, false)
    }

    fn store_snapshot_reference_records_from(
        &self,
        start: usize,
        local_side_columns: bool,
    ) -> Result<Vec<SnapshotReferenceRecord>, SnapshotCodecError> {
        let store = &self.store;
        let mut references = Vec::new();
        for raw_owner in start..store.len() {
            let owner = TypeId(
                u32::try_from(raw_owner).map_err(|_| validation("snapshot TypeId exceeds u32"))?,
            );

            match store.tag(owner) {
                TypeTag::Intrinsic | TypeTag::Literal | TypeTag::Infer | TypeTag::MappedValue => {}
                TypeTag::Object => {
                    let object = store
                        .object_type(owner)
                        .ok_or_else(|| validation("validated object payload is missing"))?;
                    for property in &object.properties {
                        push_type_operand(&mut references, owner, property.ty);
                        if let Some(write_ty) = property.write_ty {
                            push_type_operand(&mut references, owner, write_ty);
                        }
                        if let Some(class) = property.declaring_class {
                            push_class_identity(
                                &mut references,
                                owner,
                                DECLARING_CLASS_FIELD,
                                class,
                            );
                        }
                    }
                    if let Some(index) = object.string_index {
                        push_type_operand(&mut references, owner, index);
                    }
                    if let Some(index) = object.number_index {
                        push_type_operand(&mut references, owner, index);
                    }
                    for &signature in &object.call_signatures {
                        push_type_operand(&mut references, owner, signature);
                    }
                    for &signature in &object.construct_signatures {
                        push_type_operand(&mut references, owner, signature);
                    }
                }
                TypeTag::Union => {
                    for &member in store
                        .union_members(owner)
                        .ok_or_else(|| validation("validated union payload is missing"))?
                    {
                        push_type_operand(&mut references, owner, member);
                    }
                }
                TypeTag::Intersection => {
                    for &member in store
                        .intersection_members(owner)
                        .ok_or_else(|| validation("validated intersection payload is missing"))?
                    {
                        push_type_operand(&mut references, owner, member);
                    }
                }
                TypeTag::Function => {
                    let function = store
                        .function_type(owner)
                        .ok_or_else(|| validation("validated function payload is missing"))?;
                    for parameter in &function.type_params {
                        push_type_param_identity(&mut references, owner, parameter.id);
                        if let Some(constraint) = parameter.constraint {
                            references.push(reference(
                                TYPE_DOMAIN,
                                TYPE_DOMAIN,
                                CONSTRAINT_FIELD,
                                owner.0,
                                constraint.0,
                            ));
                        }
                        if let Some(default) = parameter.default {
                            references.push(reference(
                                TYPE_DOMAIN,
                                TYPE_DOMAIN,
                                DEFAULT_FIELD,
                                owner.0,
                                default.0,
                            ));
                        }
                    }
                    if let Some(receiver) = function.receiver {
                        push_type_operand(&mut references, owner, receiver);
                    }
                    for parameter in &function.params {
                        push_type_operand(&mut references, owner, parameter.ty);
                    }
                    push_type_operand(&mut references, owner, function.ret);
                }
                TypeTag::TypeParam => {
                    push_type_param_identity(
                        &mut references,
                        owner,
                        store
                            .type_param(owner)
                            .ok_or_else(|| {
                                validation("validated type parameter payload is missing")
                            })?
                            .id,
                    );
                }
                TypeTag::Array => {
                    push_type_operand(
                        &mut references,
                        owner,
                        store
                            .array_type(owner)
                            .ok_or_else(|| validation("validated array payload is missing"))?
                            .element,
                    );
                }
                TypeTag::Tuple => {
                    let tuple = store
                        .tuple_type(owner)
                        .ok_or_else(|| validation("validated tuple payload is missing"))?;
                    for &element in &tuple.elements {
                        push_type_operand(&mut references, owner, element);
                    }
                    if let Some(rest) = tuple.rest {
                        push_type_operand(&mut references, owner, rest.ty);
                    }
                }
                TypeTag::Readonly => {
                    push_type_operand(
                        &mut references,
                        owner,
                        store
                            .readonly_operand(owner)
                            .ok_or_else(|| validation("validated readonly payload is missing"))?,
                    );
                }
                TypeTag::Conditional => {
                    let conditional = store
                        .conditional_type(owner)
                        .ok_or_else(|| validation("validated conditional payload is missing"))?;
                    for target in [
                        conditional.check,
                        conditional.extends_ty,
                        conditional.true_branch,
                        conditional.false_branch,
                    ] {
                        push_type_operand(&mut references, owner, target);
                    }
                }
                TypeTag::Instantiation => {
                    let instantiation = store
                        .instantiation_type(owner)
                        .ok_or_else(|| validation("validated instantiation payload is missing"))?;
                    push_type_operand(&mut references, owner, instantiation.base);
                    for &(parameter, argument) in &instantiation.args {
                        push_type_param_identity(&mut references, owner, parameter);
                        push_type_operand(&mut references, owner, argument);
                    }
                }
                TypeTag::Mapped => {
                    let mapped = store
                        .mapped_type(owner)
                        .ok_or_else(|| validation("validated mapped payload is missing"))?;
                    push_type_operand(&mut references, owner, mapped.key_source);
                    push_type_operand(&mut references, owner, mapped.value_template);
                    if let Some(source) = mapped.modifiers_source {
                        push_type_operand(&mut references, owner, source);
                    }
                }
                TypeTag::Template => {
                    for &hole in &store
                        .template_type(owner)
                        .ok_or_else(|| validation("validated template payload is missing"))?
                        .holes
                    {
                        push_type_operand(&mut references, owner, hole);
                    }
                }
                TypeTag::Keyof => {
                    push_type_operand(
                        &mut references,
                        owner,
                        store
                            .keyof_operand(owner)
                            .ok_or_else(|| validation("validated keyof payload is missing"))?,
                    );
                }
                TypeTag::ClassInstance => {
                    let instance = store
                        .class_instance_type(owner)
                        .ok_or_else(|| validation("validated class instance payload is missing"))?;
                    push_class_identity(
                        &mut references,
                        owner,
                        CLASS_IDENTITY_FIELD,
                        instance.class,
                    );
                    for &argument in &instance.args {
                        push_type_operand(&mut references, owner, argument);
                    }
                }
                TypeTag::DeferredIndexedAccess => {
                    let access = store.deferred_indexed_access_type(owner).ok_or_else(|| {
                        validation("validated deferred indexed access payload is missing")
                    })?;
                    push_type_operand(&mut references, owner, access.object);
                    push_type_operand(&mut references, owner, access.index);
                }
            }
        }

        let constraints = if local_side_columns {
            store
                .local_type_param_constraints_for_test()
                .collect::<Vec<_>>()
        } else {
            store.snapshot_type_param_constraints()
        };
        for (index, (parameter, constraint)) in constraints.into_iter().enumerate() {
            let owner =
                u32::try_from(index).map_err(|_| validation("constraint row index exceeds u32"))?;
            references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_PARAM_DOMAIN,
                CONSTRAINT_OWNER_FIELD,
                owner,
                parameter.0,
            ));
            references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                CONSTRAINT_TARGET_FIELD,
                owner,
                constraint.0,
            ));
        }
        let frozen_parameters = if local_side_columns {
            store
                .local_frozen_type_params_for_test()
                .collect::<Vec<_>>()
        } else {
            store.snapshot_frozen_type_params()
        };
        for (index, parameter) in frozen_parameters.into_iter().enumerate() {
            references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_PARAM_DOMAIN,
                FROZEN_TYPE_PARAM_FIELD,
                u32::try_from(index)
                    .map_err(|_| validation("frozen parameter index exceeds u32"))?,
                parameter.0,
            ));
        }
        let mut template_names = if local_side_columns {
            store.local_template_name_ids_for_test().collect::<Vec<_>>()
        } else {
            store.snapshot_template_name_ids().collect::<Vec<_>>()
        };
        template_names.sort_unstable();
        for (index, id) in template_names.into_iter().enumerate() {
            references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                TEMPLATE_NAME_TYPE_FIELD,
                u32::try_from(index)
                    .map_err(|_| validation("template-name row index exceeds u32"))?,
                id.0,
            ));
        }

        references.sort_unstable();
        Ok(references)
    }

    #[cfg(test)]
    fn write_identity_snapshot_for_test(
        &self,
        writer: &mut SnapshotWriter,
    ) -> Result<(), SnapshotCodecError> {
        writer.u32(VERSION);
        let mut buckets = self.dedup_buckets().collect::<Vec<_>>();
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

        let mut reserved = self.reserved_types().collect::<Vec<_>>();
        reserved.sort_by_key(|(id, _)| id.0);
        writer.usize(reserved.len())?;
        for (id, terminal) in reserved {
            if terminal.state != ReservedTypeState::Frozen {
                return Err(SnapshotCodecError::invalid(
                    0,
                    format!(
                        "snapshot cannot expose pending reserved type {} ({:?}, name={:?})",
                        id.0,
                        terminal.kind,
                        self.store.template_name(*id)
                    ),
                ));
            }
            writer.u32(id.0);
            writer.u8(reserved_kind_discriminant(terminal.kind));
        }

        for id in well_known_ids(self.well_known) {
            writer.u32(id.0);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn write_snapshot_for_test(
        &self,
        writer: &mut SnapshotWriter,
    ) -> Result<(), SnapshotCodecError> {
        writer.u32(VERSION);
        self.store.write_snapshot_for_test(writer)?;
        self.write_identity_snapshot_for_test(writer)
    }

    #[cfg(test)]
    pub(crate) fn encode_split_snapshot_sections_for_test(
        &self,
    ) -> Result<(Vec<u8>, Vec<u8>), SnapshotCodecError> {
        let mut store = SnapshotWriter::new();
        self.store.write_snapshot_for_test(&mut store)?;
        let mut identity = SnapshotWriter::new();
        self.write_identity_snapshot_for_test(&mut identity)?;
        Ok((store.into_bytes(), identity.into_bytes()))
    }

    #[cfg(test)]
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
        let store = Store::read_snapshot(reader)?;

        let identity_version_offset = reader.position();
        if reader.u32()? != VERSION {
            return Err(SnapshotCodecError::invalid(
                identity_version_offset,
                "unsupported interner identity snapshot version",
            ));
        }

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

        let graph_identity = Arc::clone(store.semantic_graph_identity());
        let interner = Interner {
            store,
            free_param_summaries: FreeParamSummaryCache::new(Arc::clone(&graph_identity)),
            clean_application_results: CleanApplicationResultCache::new(graph_identity),
            dedup_base: Arc::new(FxHashMap::default()),
            dedup,
            reserved_types_base: Arc::new(FxHashMap::default()),
            reserved_types,
            well_known,
            #[cfg(test)]
            user_delta_drop_witness: None,
        };
        interner.validate_snapshot()?;
        Ok(interner)
    }

    fn read_identity_snapshot(
        store: Store,
        reader: &mut SnapshotReader<'_>,
    ) -> Result<Self, SnapshotCodecError> {
        let version_offset = reader.position();
        if reader.u32()? != VERSION {
            return Err(SnapshotCodecError::invalid(
                version_offset,
                "unsupported interner identity snapshot version",
            ));
        }

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
        let graph_identity = Arc::clone(store.semantic_graph_identity());
        let interner = Interner {
            store,
            free_param_summaries: FreeParamSummaryCache::new(Arc::clone(&graph_identity)),
            clean_application_results: CleanApplicationResultCache::new(graph_identity),
            dedup_base: Arc::new(FxHashMap::default()),
            dedup,
            reserved_types_base: Arc::new(FxHashMap::default()),
            reserved_types,
            well_known: well_known_from_ids(ids),
            #[cfg(test)]
            user_delta_drop_witness: None,
        };
        interner.validate_snapshot()?;
        Ok(interner)
    }

    pub(crate) fn decode_split_snapshot_sections(
        store_bytes: &[u8],
        identity_bytes: &[u8],
    ) -> Result<Self, SnapshotCodecError> {
        let mut store_reader = SnapshotReader::new(store_bytes);
        let store = Store::read_snapshot(&mut store_reader)?;
        store_reader.finish()?;
        let mut identity_reader = SnapshotReader::new(identity_bytes);
        let interner = Self::read_identity_snapshot(store, &mut identity_reader)?;
        identity_reader.finish()?;
        Ok(interner)
    }

    #[cfg(test)]
    pub(crate) fn encode_snapshot_bytes_for_test(&self) -> Result<Vec<u8>, SnapshotCodecError> {
        let mut writer = SnapshotWriter::new();
        self.write_snapshot_for_test(&mut writer)?;
        Ok(writer.into_bytes())
    }

    #[cfg(test)]
    pub(crate) fn decode_snapshot_bytes_for_test(bytes: &[u8]) -> Result<Self, SnapshotCodecError> {
        let mut reader = SnapshotReader::new(bytes);
        let interner = Self::read_snapshot_for_test(&mut reader)?;
        reader.finish()?;
        Ok(interner)
    }

    fn validate_snapshot(&self) -> Result<(), SnapshotCodecError> {
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

        for (id, reserved) in self.reserved_types() {
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
        for id in self.store.snapshot_template_name_ids() {
            if !self.contains_reserved_type(id) {
                return Err(validation(
                    "template display name is not attached to a reservation",
                ));
            }
        }

        let mut seen = vec![false; len];
        for (hash, candidates) in self.dedup_buckets() {
            if candidates.is_empty() {
                return Err(validation("dedup bucket is empty"));
            }
            for (position, candidate) in candidates.iter().copied().enumerate() {
                if candidate.index() >= len
                    || self.contains_reserved_type(candidate)
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
            if covered == self.contains_reserved_type(id) {
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
        well_known.object,
    ]
}

fn well_known_from_ids(ids: [TypeId; WELL_KNOWN_COUNT]) -> WellKnown {
    let [error, any, unknown, never, void, null, undefined, boolean, number, string, uppercase, lowercase, capitalize, uncapitalize, this_type, omit_this_parameter, object] =
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
        object,
    }
}

#[cfg(test)]
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
        ClassId, ConditionalType, FunctionType, GenericTypeParam, MappedType, ModifierOp,
        ObjectType, ParameterType, PropertyType, TemplateType, TupleRestType, TupleType,
        Visibility,
    };

    struct RichFixture {
        interner: Interner,
        literal: TypeId,
        parameter_id: TypeParamId,
        parameter: TypeId,
        array: TypeId,
        readonly: TypeId,
        tuple: TypeId,
        function: TypeId,
        object: TypeId,
        conditional: TypeId,
        instantiation: TypeId,
        mapped_value: TypeId,
        mapped: TypeId,
        template: TypeId,
        union: TypeId,
        intersection: TypeId,
        keyof: TypeId,
        class_instance: TypeId,
        indexed: TypeId,
    }

    fn rich_fixture() -> RichFixture {
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
        let readonly = interner.intern_readonly(array);
        let tuple = interner.intern_tuple_type(TupleType::with_rest(
            vec![literal],
            TupleRestType::new(1, array),
        ));
        let function = interner.intern_function(FunctionType {
            type_params: vec![GenericTypeParam {
                id: parameter_id,
                constraint: Some(wk.string),
                default: Some(literal),
            }],
            receiver: Some(parameter),
            params: vec![ParameterType::required("value", tuple)],
            ret: wk.boolean,
        });
        let object = interner.reserve_object();
        let class_property = PropertyType {
            name: "classy".to_owned(),
            ty: literal,
            write_ty: Some(wk.string),
            optional: false,
            visibility: Visibility::Protected,
            declaring_class: Some(ClassId(12)),
            readonly: true,
            is_accessor: true,
        };
        interner.fill_object(
            object,
            ObjectType {
                properties: vec![class_property, PropertyType::public("next", object)],
                string_index: Some(literal),
                number_index: Some(wk.number),
                call_signatures: vec![function],
                construct_signatures: vec![function],
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
        let instantiation =
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
        let template = interner.intern_template(TemplateType {
            texts: vec!["before".to_owned(), "after".to_owned()],
            holes: vec![literal],
        });
        let union = interner.union(vec![literal, wk.string]);
        let intersection = interner.intersection(vec![object, mapped]);
        let keyof = interner.intern_keyof(parameter);
        let class_instance = interner.intern_class_instance(ClassId(11), vec![literal]);
        let indexed = interner.intern_deferred_indexed_access(class_instance, parameter);
        RichFixture {
            interner,
            literal,
            parameter_id,
            parameter,
            array,
            readonly,
            tuple,
            function,
            object,
            conditional,
            instantiation,
            mapped_value,
            mapped,
            template,
            union,
            intersection,
            keyof,
            class_instance,
            indexed,
        }
    }

    fn rich_interner() -> Interner {
        rich_fixture().interner
    }

    #[test]
    fn reference_manifest_exactly_enumerates_rich_type_universe() {
        let fixture = rich_fixture();
        let wk = fixture.interner.well_known();
        let (store_references, interner_references) =
            fixture.interner.snapshot_reference_records_for_test();

        let type_ref = |owner: TypeId, target: TypeId| {
            reference(
                TYPE_DOMAIN,
                TYPE_DOMAIN,
                TYPE_OPERAND_FIELD,
                owner.0,
                target.0,
            )
        };
        let parameter_ref = |owner: TypeId, target: TypeParamId| {
            reference(
                TYPE_DOMAIN,
                TYPE_PARAM_DOMAIN,
                TYPE_PARAM_IDENTITY_FIELD,
                owner.0,
                target.0,
            )
        };
        let mut expected_store = vec![
            parameter_ref(fixture.parameter, fixture.parameter_id),
            type_ref(fixture.array, fixture.parameter),
            type_ref(fixture.readonly, fixture.array),
            type_ref(fixture.tuple, fixture.literal),
            type_ref(fixture.tuple, fixture.array),
            parameter_ref(fixture.function, fixture.parameter_id),
            reference(
                TYPE_DOMAIN,
                TYPE_DOMAIN,
                CONSTRAINT_FIELD,
                fixture.function.0,
                wk.string.0,
            ),
            reference(
                TYPE_DOMAIN,
                TYPE_DOMAIN,
                DEFAULT_FIELD,
                fixture.function.0,
                fixture.literal.0,
            ),
            type_ref(fixture.function, fixture.parameter),
            type_ref(fixture.function, fixture.tuple),
            type_ref(fixture.function, wk.boolean),
            type_ref(fixture.object, fixture.literal),
            type_ref(fixture.object, wk.string),
            reference(
                TYPE_DOMAIN,
                CLASS_DOMAIN,
                DECLARING_CLASS_FIELD,
                fixture.object.0,
                12,
            ),
            type_ref(fixture.object, fixture.object),
            type_ref(fixture.object, fixture.literal),
            type_ref(fixture.object, wk.number),
            type_ref(fixture.object, fixture.function),
            type_ref(fixture.object, fixture.function),
            type_ref(fixture.conditional, fixture.parameter),
            type_ref(fixture.conditional, wk.string),
            type_ref(fixture.conditional, fixture.object),
            type_ref(fixture.conditional, wk.never),
            type_ref(fixture.instantiation, fixture.conditional),
            parameter_ref(fixture.instantiation, fixture.parameter_id),
            type_ref(fixture.instantiation, fixture.literal),
            type_ref(fixture.mapped, wk.string),
            type_ref(fixture.mapped, fixture.mapped_value),
            type_ref(fixture.mapped, fixture.object),
            type_ref(fixture.template, fixture.literal),
            type_ref(fixture.union, fixture.literal),
            type_ref(fixture.union, wk.string),
            type_ref(fixture.intersection, fixture.object),
            type_ref(fixture.intersection, fixture.mapped),
            type_ref(fixture.keyof, fixture.parameter),
            reference(
                TYPE_DOMAIN,
                CLASS_DOMAIN,
                CLASS_IDENTITY_FIELD,
                fixture.class_instance.0,
                11,
            ),
            type_ref(fixture.class_instance, fixture.literal),
            type_ref(fixture.indexed, fixture.class_instance),
            type_ref(fixture.indexed, fixture.parameter),
            reference(
                CONTAINER_DOMAIN,
                TYPE_PARAM_DOMAIN,
                CONSTRAINT_OWNER_FIELD,
                0,
                fixture.parameter_id.0,
            ),
            reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                CONSTRAINT_TARGET_FIELD,
                0,
                wk.string.0,
            ),
            reference(
                CONTAINER_DOMAIN,
                TYPE_PARAM_DOMAIN,
                FROZEN_TYPE_PARAM_FIELD,
                0,
                fixture.parameter_id.0,
            ),
            reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                TEMPLATE_NAME_TYPE_FIELD,
                0,
                fixture.conditional.0,
            ),
            reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                TEMPLATE_NAME_TYPE_FIELD,
                1,
                fixture.mapped.0,
            ),
        ];
        expected_store.sort_unstable();
        assert_eq!(store_references, expected_store);

        let mut expected_interner = Vec::new();
        let mut buckets = fixture.interner.dedup.iter().collect::<Vec<_>>();
        buckets.sort_unstable_by_key(|(hash, _)| **hash);
        for (index, (_, candidates)) in buckets.into_iter().enumerate() {
            let mut candidates = candidates.iter().copied().collect::<Vec<_>>();
            candidates.sort_unstable();
            expected_interner.extend(candidates.into_iter().map(|candidate| {
                reference(
                    INTERNER_BUCKET_DOMAIN,
                    TYPE_DOMAIN,
                    BUCKET_CANDIDATE_FIELD,
                    u32::try_from(index).expect("bucket index fits u32"),
                    candidate.0,
                )
            }));
        }
        for (index, id) in [fixture.object, fixture.conditional, fixture.mapped]
            .into_iter()
            .enumerate()
        {
            expected_interner.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                RESERVED_TYPE_FIELD,
                u32::try_from(index).expect("reserved index fits u32"),
                id.0,
            ));
        }
        expected_interner.extend(
            well_known_ids(wk)
                .into_iter()
                .enumerate()
                .map(|(slot, id)| {
                    reference(
                        CONTAINER_DOMAIN,
                        TYPE_DOMAIN,
                        WELL_KNOWN_TYPE_FIELD,
                        u32::try_from(slot).expect("well-known slot fits u32"),
                        id.0,
                    )
                }),
        );
        expected_interner.sort_unstable();
        assert_eq!(interner_references, expected_interner);
    }

    #[test]
    fn reference_manifest_is_canonical_and_tracks_append_only_mutation() {
        let mut interner = rich_interner();
        let before = interner.snapshot_reference_records_for_test();
        assert!(before.0.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(before.1.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(interner.snapshot_reference_records_for_test(), before);

        let wk = interner.well_known();
        let new_array = interner.intern_array(wk.string);
        let after = interner.snapshot_reference_records_for_test();
        let added_store_reference = reference(
            TYPE_DOMAIN,
            TYPE_DOMAIN,
            TYPE_OPERAND_FIELD,
            new_array.0,
            wk.string.0,
        );
        let mut expected_store = before.0;
        expected_store.push(added_store_reference);
        expected_store.sort_unstable();
        assert_eq!(after.0, expected_store);
        assert!(after.1.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(after.1.iter().any(|record| {
            record.0 == INTERNER_BUCKET_DOMAIN
                && record.1 == TYPE_DOMAIN
                && record.2 == BUCKET_CANDIDATE_FIELD
                && record.4 == new_array.0
        }));
        assert!(after
            .0
            .iter()
            .chain(&after.1)
            .all(|record| record.0 <= 31 && record.1 <= 31 && record.2 <= 31));
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
    fn split_type_snapshot_roundtrips_and_rejects_cross_section_corruption() {
        let interner = rich_interner();
        let (store, identity) = interner
            .encode_split_snapshot_sections_for_test()
            .expect("split type universe encodes");
        let decoded = Interner::decode_split_snapshot_sections(&store, &identity)
            .expect("split type universe decodes");
        assert_eq!(
            decoded
                .encode_split_snapshot_sections_for_test()
                .expect("decoded split universe re-encodes"),
            (store.clone(), identity.clone())
        );

        let mut bad_store = store.clone();
        bad_store[0] ^= 0xff;
        assert!(Interner::decode_split_snapshot_sections(&bad_store, &identity).is_err());
        let mut bad_identity = identity;
        bad_identity[0] ^= 0xff;
        assert!(Interner::decode_split_snapshot_sections(&store, &bad_identity).is_err());
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
