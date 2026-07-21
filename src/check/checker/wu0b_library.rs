//! Measurement-only compiler for injected declaration-library profiles.

use super::context::DeclTypes;
use super::events_library::{
    LibraryEventKey, LibraryEventLedger, LibraryEventLedgerError, LibraryRecordTicket,
    LibrarySemanticReportingAdapter,
};
use super::lexical_events::LexicalReservations;
use super::lexical_events_library::{library_unit, ExactUnit};
#[cfg(test)]
use super::library_reporting::LibraryReportingFamily;
use super::library_reporting::{LibraryReportingConsumer, LibraryReportingReceipt};
use super::namespace_values::NamespaceValueRegistry;
use super::reporting_record::CheckerRecord;
use super::type_groups::{
    InterfaceAlternativeKind, PublishedTypeEnvironment, PublishedTypeGroupSurface,
    PublishedTypeGroupTerminal, PublishedTypeParameterDefault, TypeGroupUnavailableCause,
};
use super::wu0e_diagnostic::{DiagnosticBoundaryAdapter, DiagnosticPhase};
use super::{
    build_pass_with_tickets, finish_semantic_effects, reserve_type_decls, PassReporting,
    PassReportingPlan,
};
use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::{TypeGroupId, ValueStorageId};
use crate::binder::namespace::{
    exact_key, CompilationUnit, ExactKey, ExportContextKind, ExportSyntaxDisposition,
    MergeDeclarationKind, ModuleBindingContext, SourceFileKind,
};
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::class_semantics::{CanonicalPublishedClassTerminal, DemandOutcome, PublishedClasses};
use crate::diagnostics::{render_to_writer_with_format, render_type, DiagnosticFormat};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use crate::span::Span;
use crate::types::repr::{ClassId, IntrinsicKind, LiteralValue, ModifierOp, TypeTag, Visibility};
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::time::{Duration, Instant};

#[derive(Copy, Clone, Debug)]
pub(crate) struct InjectedLibrarySource<'source> {
    pub(crate) file_ordinal: LibraryFileOrdinal,
    pub(crate) name: &'source str,
    pub(crate) source: &'source str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InjectedProfileError {
    EmptyProfile,
    EmptyName {
        file_ordinal: LibraryFileOrdinal,
    },
    DuplicateName(String),
    DuplicateFileOrdinal(LibraryFileOrdinal),
    SourceKeyOverflow,
    Parse {
        file_ordinal: LibraryFileOrdinal,
        messages: Vec<String>,
    },
    Reservation(String),
    Reporting(LibraryEventLedgerError),
    CanonicalProjection(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wu0dSemanticComponents {
    pub(crate) diagnostics: Vec<u8>,
    pub(crate) incomplete: Vec<u8>,
    pub(crate) library_ledger: Vec<u8>,
    pub(crate) frozen_library_product: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wu0dSemanticComponentIdentity {
    pub(crate) byte_len: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wu0dSemanticIdentity {
    pub(crate) diagnostics: Wu0dSemanticComponentIdentity,
    pub(crate) incomplete: Wu0dSemanticComponentIdentity,
    pub(crate) library_ledger: Wu0dSemanticComponentIdentity,
    pub(crate) frozen_library_product: Wu0dSemanticComponentIdentity,
    pub(crate) aggregate_sha256: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Wu0dFrozenProductSection {
    SourceRecords,
    TypeStore,
    TypePublications,
    GlobalValues,
    ModuleValues,
    NamespaceValues,
    Classes,
}

impl Wu0dFrozenProductSection {
    const ALL: [Self; 7] = [
        Self::SourceRecords,
        Self::TypeStore,
        Self::TypePublications,
        Self::GlobalValues,
        Self::ModuleValues,
        Self::NamespaceValues,
        Self::Classes,
    ];

    const fn tag(self) -> u8 {
        match self {
            Self::SourceRecords => 1,
            Self::TypeStore => 2,
            Self::TypePublications => 3,
            Self::GlobalValues => 4,
            Self::ModuleValues => 5,
            Self::NamespaceValues => 6,
            Self::Classes => 7,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Wu0dDecodedClassTerminal {
    Ready,
    HeritagePoison,
    InitializerPoison,
    SurfacePoison,
}

type Wu0dClassDecodeResult = Result<Vec<(ClassId, Wu0dDecodedClassTerminal)>, String>;

struct Wu0dFrozenProductSectionDescriptor {
    section: Wu0dFrozenProductSection,
    range: Range<usize>,
}

struct Wu0dFrozenLibraryProduct {
    bytes: Vec<u8>,
    sections: Vec<Wu0dFrozenProductSectionDescriptor>,
}

fn semantic_component_identity(bytes: &[u8]) -> Wu0dSemanticComponentIdentity {
    Wu0dSemanticComponentIdentity {
        byte_len: u64::try_from(bytes.len()).expect("owned semantic component length fits u64"),
        sha256: format!("{:x}", Sha256::digest(bytes)),
    }
}

pub(crate) fn canonical_wu0d_semantic_identity(
    components: &Wu0dSemanticComponents,
) -> Wu0dSemanticIdentity {
    let ordered = [
        components.diagnostics.as_slice(),
        components.incomplete.as_slice(),
        components.library_ledger.as_slice(),
        components.frozen_library_product.as_slice(),
    ];
    let mut aggregate = Sha256::new();
    aggregate.update(b"typokat-wu0d-semantic-v1");
    for component in ordered {
        let length = u64::try_from(component.len()).expect("semantic component length fits u64");
        aggregate.update(length.to_be_bytes());
        aggregate.update(component);
    }
    Wu0dSemanticIdentity {
        diagnostics: semantic_component_identity(&components.diagnostics),
        incomplete: semantic_component_identity(&components.incomplete),
        library_ledger: semantic_component_identity(&components.library_ledger),
        frozen_library_product: semantic_component_identity(&components.frozen_library_product),
        aggregate_sha256: format!("{:x}", aggregate.finalize()),
    }
}

pub(crate) fn canonical_wu0d_semantic_identity_from_components_for_test(
    components: &Wu0dSemanticComponents,
) -> Wu0dSemanticIdentity {
    canonical_wu0d_semantic_identity(components)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryPhaseCounts {
    pub(crate) parse_units: usize,
    pub(crate) bind_units: usize,
    pub(crate) reserved_records: usize,
    pub(crate) filled_records: usize,
    pub(crate) publication_validations: usize,
    pub(crate) statement_check_units: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryPhaseTimings {
    pub(crate) parse: Duration,
    pub(crate) bind: Duration,
    pub(crate) reserve_fill: Duration,
    pub(crate) publication_validation: Duration,
    pub(crate) statement_check: Duration,
}

impl LibraryPhaseTimings {
    fn measured_total(&self) -> Duration {
        self.parse
            + self.bind
            + self.reserve_fill
            + self.publication_validation
            + self.statement_check
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeProbe {
    pub(crate) identity: TypeGroupId,
    pub(crate) declaration_identities: Vec<(LibraryFileOrdinal, TypeGroupId)>,
    pub(crate) declaration_count: usize,
    pub(crate) member_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SignatureProbe {
    pub(crate) parameter_types: Vec<String>,
    pub(crate) return_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallableMemberProbe {
    pub(crate) name: String,
    pub(crate) identity: ValueStorageId,
    pub(crate) source: ExactUnit,
    pub(crate) reservation_source: ExactUnit,
    pub(crate) source_start: u32,
    pub(crate) call_signature_count: usize,
    pub(crate) signatures: Vec<SignatureProbe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValueProbe {
    pub(crate) identity: ValueStorageId,
    visible_type: Option<TypeId>,
    pub(crate) participant_identities: Vec<(LibraryFileOrdinal, ValueStorageId)>,
    pub(crate) declaration_count: usize,
    pub(crate) call_signature_count: usize,
    pub(crate) member_names: Vec<String>,
    pub(crate) callable_members: Vec<CallableMemberProbe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NamespaceValueProbe {
    namespace: u32,
    name: String,
    value: ValueProbe,
}

#[derive(Debug)]
pub(crate) struct InjectedProfileRun {
    pub(crate) phase_counts: LibraryPhaseCounts,
    pub(crate) phase_timings: LibraryPhaseTimings,
    pub(crate) reserved_file_ordinals: Vec<LibraryFileOrdinal>,
    pub(crate) reporting_receipts: Vec<LibraryReportingReceipt>,
    pub(crate) library_records: Vec<(LibraryEventKey, CheckerRecord)>,
    pub(crate) pass_source_units: Vec<ExactUnit>,
    pub(crate) lexical_source_units: Vec<ExactUnit>,
    pub(crate) wu0d_semantic_components: Wu0dSemanticComponents,
    global_types: BTreeMap<String, TypeProbe>,
    module_types: BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
    global_values: BTreeMap<String, ValueProbe>,
}

impl InjectedProfileRun {
    pub(crate) fn global_type_probe(&self, name: &str) -> Option<&TypeProbe> {
        self.global_types.get(name)
    }

    pub(crate) fn module_type_probe(
        &self,
        file_ordinal: LibraryFileOrdinal,
        name: &str,
    ) -> Option<&TypeProbe> {
        self.module_types.get(&(file_ordinal, name.to_owned()))
    }

    pub(crate) fn global_value_probe(&self, name: &str) -> Option<&ValueProbe> {
        self.global_values.get(name)
    }

    pub(crate) fn wu0d_frozen_product_section_for_test(
        &self,
        section: Wu0dFrozenProductSection,
    ) -> &[u8] {
        let product = &self.wu0d_semantic_components.frozen_library_product;
        let range = parse_frozen_product_section_range(product, section)
            .expect("owned WU0D frozen product has canonical sections");
        &product[range]
    }

    pub(crate) fn wu0d_decoded_class_terminals_for_test(
        &self,
    ) -> Result<Vec<(crate::types::repr::ClassId, Wu0dDecodedClassTerminal)>, String> {
        let section = self.wu0d_frozen_product_section_for_test(Wu0dFrozenProductSection::Classes);
        decode_wu0d_class_terminals_for_test(section)
    }
}

struct CanonicalInput<'source> {
    file_ordinal: LibraryFileOrdinal,
    name: &'source str,
    source: &'source str,
    kind: SourceFileKind,
    source_key: ExactKey,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ParserExportClaim {
    file_ordinal: LibraryFileOrdinal,
    span: Span,
}

struct CanonicalBytes(Vec<u8>);

impl CanonicalBytes {
    fn domain(domain: &[u8]) -> Self {
        Self(Vec::from(domain))
    }

    fn byte(&mut self, value: u8) {
        self.0.push(value);
    }

    fn bool(&mut self, value: bool) {
        self.byte(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn usize(&mut self, value: usize) -> Result<(), InjectedProfileError> {
        self.u64(u64::try_from(value).map_err(|_| {
            InjectedProfileError::CanonicalProjection("usize does not fit u64".to_owned())
        })?);
        Ok(())
    }

    fn bytes(&mut self, value: &[u8]) -> Result<(), InjectedProfileError> {
        self.usize(value.len())?;
        self.0.extend_from_slice(value);
        Ok(())
    }

    fn string(&mut self, value: &str) -> Result<(), InjectedProfileError> {
        self.bytes(value.as_bytes())
    }

    fn type_id(&mut self, value: TypeId) {
        self.u32(value.0);
    }

    fn optional_type_id(&mut self, value: Option<TypeId>) {
        self.bool(value.is_some());
        if let Some(value) = value {
            self.type_id(value);
        }
    }

    fn type_ids(&mut self, values: &[TypeId]) -> Result<(), InjectedProfileError> {
        self.usize(values.len())?;
        for value in values {
            self.type_id(*value);
        }
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }
}

fn intrinsic_code(kind: IntrinsicKind) -> u8 {
    match kind {
        IntrinsicKind::Error => 0,
        IntrinsicKind::Any => 1,
        IntrinsicKind::Unknown => 2,
        IntrinsicKind::Never => 3,
        IntrinsicKind::Void => 4,
        IntrinsicKind::Null => 5,
        IntrinsicKind::Undefined => 6,
        IntrinsicKind::Boolean => 7,
        IntrinsicKind::Number => 8,
        IntrinsicKind::String => 9,
        IntrinsicKind::Uppercase => 10,
        IntrinsicKind::Lowercase => 11,
        IntrinsicKind::Capitalize => 12,
        IntrinsicKind::Uncapitalize => 13,
        IntrinsicKind::ThisType => 14,
        IntrinsicKind::OmitThisParameter => 15,
    }
}

fn visibility_code(visibility: Visibility) -> u8 {
    match visibility {
        Visibility::Public => 0,
        Visibility::Private => 1,
        Visibility::Protected => 2,
    }
}

fn modifier_code(modifier: ModifierOp) -> u8 {
    match modifier {
        ModifierOp::Keep => 0,
        ModifierOp::Add => 1,
        ModifierOp::Remove => 2,
    }
}

fn encode_store_row(
    bytes: &mut CanonicalBytes,
    store: &Store,
    id: TypeId,
) -> Result<(), InjectedProfileError> {
    let tag = store.tag(id);
    bytes.type_id(id);
    bytes.byte(tag.discriminant());
    bytes.u32(store.flags(id).0);
    match tag {
        TypeTag::Intrinsic => bytes.byte(intrinsic_code(store.intrinsic_kind(id).ok_or_else(
            || InjectedProfileError::CanonicalProjection("missing intrinsic payload".to_owned()),
        )?)),
        TypeTag::Literal => match store.literal_value(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing literal payload".to_owned())
        })? {
            LiteralValue::Number(value) => {
                bytes.byte(0);
                bytes.u64(value.to_bits());
            }
            LiteralValue::String(value) => {
                bytes.byte(1);
                bytes.string(value)?;
            }
            LiteralValue::Boolean(value) => {
                bytes.byte(2);
                bytes.bool(*value);
            }
        },
        TypeTag::Object => {
            let object = store.object_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing object payload".to_owned())
            })?;
            bytes.usize(object.properties.len())?;
            for property in &object.properties {
                bytes.string(&property.name)?;
                bytes.type_id(property.ty);
                bytes.optional_type_id(property.write_ty);
                bytes.bool(property.optional);
                bytes.byte(visibility_code(property.visibility));
                bytes.bool(property.declaring_class.is_some());
                if let Some(class) = property.declaring_class {
                    bytes.u32(class.0);
                }
                bytes.bool(property.readonly);
                bytes.bool(property.is_accessor);
            }
            bytes.optional_type_id(object.string_index);
            bytes.optional_type_id(object.number_index);
            bytes.type_ids(&object.call_signatures)?;
            bytes.type_ids(&object.construct_signatures)?;
        }
        TypeTag::Union => bytes.type_ids(store.union_members(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing union payload".to_owned())
        })?)?,
        TypeTag::Intersection => {
            bytes.type_ids(store.intersection_members(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing intersection payload".to_owned())
            })?)?
        }
        TypeTag::Function => {
            let function = store.function_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing function payload".to_owned())
            })?;
            bytes.usize(function.type_params.len())?;
            for parameter in &function.type_params {
                bytes.u32(parameter.id.0);
                bytes.optional_type_id(parameter.constraint);
                bytes.optional_type_id(parameter.default);
            }
            bytes.optional_type_id(function.receiver);
            bytes.usize(function.params.len())?;
            for parameter in &function.params {
                bytes.string(&parameter.name)?;
                bytes.type_id(parameter.ty);
                bytes.bool(parameter.optional);
                bytes.bool(parameter.has_default);
                bytes.bool(parameter.rest);
            }
            bytes.type_id(function.ret);
        }
        TypeTag::TypeParam => {
            let parameter = store.type_param(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing type parameter".to_owned())
            })?;
            bytes.u32(parameter.id.0);
            bytes.string(&parameter.name)?;
            bytes.optional_type_id(store.type_param_constraint(parameter.id));
            bytes.bool(store.type_param_metadata_is_frozen(parameter.id));
        }
        TypeTag::Array => bytes.type_id(
            store
                .array_type(id)
                .ok_or_else(|| {
                    InjectedProfileError::CanonicalProjection("missing array payload".to_owned())
                })?
                .element,
        ),
        TypeTag::Tuple => {
            let tuple = store.tuple_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing tuple payload".to_owned())
            })?;
            bytes.type_ids(&tuple.elements)?;
            bytes.bool(tuple.rest.is_some());
            if let Some(rest) = tuple.rest {
                bytes.usize(rest.position)?;
                bytes.type_id(rest.ty);
            }
        }
        TypeTag::Readonly => bytes.type_id(store.readonly_operand(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing readonly operand".to_owned())
        })?),
        TypeTag::Conditional => {
            let conditional = store.conditional_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing conditional".to_owned())
            })?;
            bytes.type_id(conditional.check);
            bytes.type_id(conditional.extends_ty);
            bytes.type_id(conditional.true_branch);
            bytes.type_id(conditional.false_branch);
            bytes.u32(conditional.infer_count);
            bytes.bool(conditional.distributive);
            bytes.bool(conditional.poisoned);
        }
        TypeTag::Instantiation => {
            let instantiation = store.instantiation_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing instantiation".to_owned())
            })?;
            bytes.type_id(instantiation.base);
            bytes.usize(instantiation.args.len())?;
            for (parameter, argument) in &instantiation.args {
                bytes.u32(parameter.0);
                bytes.type_id(*argument);
            }
        }
        TypeTag::Infer => bytes.u32(store.infer_index(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing infer index".to_owned())
        })?),
        TypeTag::Mapped => {
            let mapped = store.mapped_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing mapped payload".to_owned())
            })?;
            bytes.bool(mapped.homomorphic);
            bytes.type_id(mapped.key_source);
            bytes.type_id(mapped.value_template);
            bytes.optional_type_id(mapped.modifiers_source);
            bytes.byte(modifier_code(mapped.optional_modifier));
            bytes.byte(modifier_code(mapped.readonly_modifier));
        }
        TypeTag::MappedValue => {}
        TypeTag::Template => {
            let template = store.template_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing template payload".to_owned())
            })?;
            bytes.usize(template.texts.len())?;
            for text in &template.texts {
                bytes.string(text)?;
            }
            bytes.type_ids(&template.holes)?;
        }
        TypeTag::Keyof => bytes.type_id(store.keyof_operand(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing keyof operand".to_owned())
        })?),
        TypeTag::ClassInstance => {
            let instance = store.class_instance_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing class instance".to_owned())
            })?;
            bytes.u32(instance.class.0);
            bytes.type_ids(&instance.args)?;
        }
        TypeTag::DeferredIndexedAccess => {
            let access = store.deferred_indexed_access_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "missing deferred indexed access".to_owned(),
                )
            })?;
            bytes.type_id(access.object);
            bytes.type_id(access.index);
        }
    }
    bytes.bool(store.template_name(id).is_some());
    if let Some(name) = store.template_name(id) {
        bytes.string(name)?;
    }
    Ok(())
}

struct Wu0dFrozenLibraryProductInput<'input> {
    source_records: &'input [CanonicalInput<'input>],
    type_publications: &'input PublishedTypeEnvironment,
    global_types: &'input BTreeMap<String, TypeProbe>,
    module_types: &'input BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
    global_values: &'input BTreeMap<String, ValueProbe>,
    module_values: &'input BTreeMap<(LibraryFileOrdinal, String), ValueProbe>,
    namespace_values: &'input [NamespaceValueProbe],
    classes: &'input PublishedClasses,
    store: &'input Store,
}

fn canonical_frozen_library_product(
    input: Wu0dFrozenLibraryProductInput<'_>,
) -> Result<Wu0dFrozenLibraryProduct, InjectedProfileError> {
    let Wu0dFrozenLibraryProductInput {
        source_records: input_source_records,
        type_publications: input_type_publications,
        global_types,
        module_types,
        global_values: input_global_values,
        module_values: input_module_values,
        namespace_values: input_namespace_values,
        classes: input_classes,
        store: input_store,
    } = input;
    let source_records = input_source_records;
    let type_publications = input_type_publications;
    let global_values = input_global_values;
    let module_values = input_module_values;
    let namespace_values = input_namespace_values;
    let classes = input_classes;
    let store = input_store;

    let mut product = Wu0dFrozenLibraryProduct {
        bytes: Vec::from(b"typokat-wu0d-frozen-library-product-v2".as_slice()),
        sections: Vec::with_capacity(Wu0dFrozenProductSection::ALL.len()),
    };
    product.bytes.push(
        u8::try_from(Wu0dFrozenProductSection::ALL.len()).map_err(|_| {
            InjectedProfileError::CanonicalProjection("section count does not fit u8".to_owned())
        })?,
    );

    let mut source_bytes = CanonicalBytes::domain(b"typokat-wu0d-source-records-v1");
    source_bytes.usize(source_records.len())?;
    for source in source_records {
        let file_ordinal = source.file_ordinal;
        let source_bytes_value = source.source.as_bytes();
        let source_sha256 = Sha256::digest(source_bytes_value);
        source_bytes.usize(file_ordinal.index())?;
        source_bytes.string(source.name)?;
        source_bytes.bytes(&source_sha256)?;
        source_bytes.bytes(source_bytes_value)?;
        source_bytes.byte(source_file_kind_code(source.kind));
        source_bytes.u32(source.source_key.0);
    }
    push_frozen_product_section(
        &mut product,
        Wu0dFrozenProductSection::SourceRecords,
        source_bytes.finish(),
    )?;

    let mut type_store = CanonicalBytes::domain(b"typokat-wu0d-type-store-v1");
    type_store.usize(store.len())?;
    for raw in 0..store.len() {
        let id = TypeId(u32::try_from(raw).map_err(|_| {
            InjectedProfileError::CanonicalProjection("type id does not fit u32".to_owned())
        })?);
        encode_store_row(&mut type_store, store, id)?;
    }
    push_frozen_product_section(
        &mut product,
        Wu0dFrozenProductSection::TypeStore,
        type_store.finish(),
    )?;

    let mut publications = CanonicalBytes::domain(b"typokat-wu0d-type-publications-v1");
    encode_type_probe_map(&mut publications, global_types)?;
    publications.usize(module_types.len())?;
    for ((file_ordinal, name), probe) in module_types {
        publications.usize(file_ordinal.index())?;
        publications.string(name)?;
        encode_type_probe(&mut publications, probe)?;
    }
    let groups = type_publications.groups();
    publications.usize(groups.len())?;
    for raw in 0..groups.len() {
        let group_id = TypeGroupId(u32::try_from(raw).map_err(|_| {
            InjectedProfileError::CanonicalProjection("group id does not fit u32".to_owned())
        })?);
        publications.u32(group_id.0);
        match groups.get(group_id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing published group".to_owned())
        })? {
            PublishedTypeGroupTerminal::Ready(group) => {
                publications.byte(0);
                publications.string(&group.name)?;
                match group.surface {
                    PublishedTypeGroupSurface::Template(template) => {
                        publications.byte(0);
                        publications.type_id(template);
                    }
                    PublishedTypeGroupSurface::Class(class) => {
                        publications.byte(1);
                        publications.u32(class.0);
                    }
                }
                publications.usize(group.parameters.len())?;
                for parameter in &group.parameters {
                    publications.u32(parameter.0);
                }
                publications.usize(group.parameter_names.len())?;
                for name in &group.parameter_names {
                    publications.string(name)?;
                }
                publications.usize(group.parameter_defaults.len())?;
                for default in &group.parameter_defaults {
                    match default {
                        PublishedTypeParameterDefault::Absent => publications.byte(0),
                        PublishedTypeParameterDefault::Ready(ty) => {
                            publications.byte(1);
                            publications.type_id(*ty);
                        }
                        PublishedTypeParameterDefault::Unsupported => publications.byte(2),
                    }
                }
                publications.usize(group.conflict_alternatives.len())?;
                for alternative in &group.conflict_alternatives {
                    publications.byte(match alternative.kind {
                        InterfaceAlternativeKind::Member => 0,
                        InterfaceAlternativeKind::StringIndex => 1,
                        InterfaceAlternativeKind::NumberIndex => 2,
                        InterfaceAlternativeKind::Heritage => 3,
                    });
                    publications.string(&alternative.key)?;
                    publications.type_ids(&alternative.types)?;
                }
            }
            PublishedTypeGroupTerminal::Unavailable(unavailable) => {
                publications.byte(1);
                publications.u32(group_id.0);
                publications.byte(match unavailable.cause {
                    TypeGroupUnavailableCause::UnsupportedComposition => 0,
                });
            }
        }
    }
    push_frozen_product_section(
        &mut product,
        Wu0dFrozenProductSection::TypePublications,
        publications.finish(),
    )?;

    let mut global_value_bytes = CanonicalBytes::domain(b"typokat-wu0d-global-values-v1");
    encode_value_probe_map(&mut global_value_bytes, global_values)?;
    push_frozen_product_section(
        &mut product,
        Wu0dFrozenProductSection::GlobalValues,
        global_value_bytes.finish(),
    )?;

    let mut module_value_bytes = CanonicalBytes::domain(b"typokat-wu0d-module-values-v1");
    module_value_bytes.usize(module_values.len())?;
    for ((file_ordinal, name), probe) in module_values {
        module_value_bytes.usize(file_ordinal.index())?;
        module_value_bytes.string(name)?;
        encode_value_probe(&mut module_value_bytes, probe)?;
    }
    push_frozen_product_section(
        &mut product,
        Wu0dFrozenProductSection::ModuleValues,
        module_value_bytes.finish(),
    )?;

    let mut namespace_value_bytes = CanonicalBytes::domain(b"typokat-wu0d-namespace-values-v1");
    namespace_value_bytes.usize(namespace_values.len())?;
    for namespace in namespace_values {
        namespace_value_bytes.u32(namespace.namespace);
        namespace_value_bytes.string(&namespace.name)?;
        encode_value_probe(&mut namespace_value_bytes, &namespace.value)?;
    }
    push_frozen_product_section(
        &mut product,
        Wu0dFrozenProductSection::NamespaceValues,
        namespace_value_bytes.finish(),
    )?;

    let canonical_classes = classes.canonical_terminals().ok_or_else(|| {
        InjectedProfileError::CanonicalProjection("class publication is not final".to_owned())
    })?;
    let mut class_bytes = CanonicalBytes::domain(b"typokat-wu0d-classes-v1");
    class_bytes.usize(canonical_classes.len())?;
    for (class, terminal) in canonical_classes {
        class_bytes.u32(class.0);
        match terminal {
            CanonicalPublishedClassTerminal::Ready(surface) => {
                class_bytes.byte(0);
                class_bytes.usize(surface.type_params().len())?;
                for parameter in surface.type_params() {
                    class_bytes.u32(parameter.0);
                }
                class_bytes.type_id(surface.instance_template());
                class_bytes.type_id(surface.static_template());
                class_bytes.optional_type_id(surface.constructor_template());
            }
            CanonicalPublishedClassTerminal::HeritagePoison => class_bytes.byte(1),
            CanonicalPublishedClassTerminal::InitializerPoison => class_bytes.byte(2),
            CanonicalPublishedClassTerminal::SurfacePoison => class_bytes.byte(3),
        }
    }
    push_frozen_product_section(
        &mut product,
        Wu0dFrozenProductSection::Classes,
        class_bytes.finish(),
    )?;

    if product.sections.len() != Wu0dFrozenProductSection::ALL.len()
        || !product
            .sections
            .iter()
            .zip(Wu0dFrozenProductSection::ALL)
            .all(|(descriptor, expected)| {
                descriptor.section == expected && descriptor.range.end <= product.bytes.len()
            })
    {
        return Err(InjectedProfileError::CanonicalProjection(
            "frozen product section table is incomplete".to_owned(),
        ));
    }
    Ok(product)
}

fn source_file_kind_code(kind: SourceFileKind) -> u8 {
    match kind {
        SourceFileKind::ImplementationTs => 0,
        SourceFileKind::ImplementationMts => 1,
        SourceFileKind::ImplementationCts => 2,
        SourceFileKind::DeclarationTs => 3,
        SourceFileKind::DeclarationMts => 4,
        SourceFileKind::DeclarationCts => 5,
    }
}

fn encode_type_probe_map(
    bytes: &mut CanonicalBytes,
    probes: &BTreeMap<String, TypeProbe>,
) -> Result<(), InjectedProfileError> {
    bytes.usize(probes.len())?;
    for (name, probe) in probes {
        bytes.string(name)?;
        encode_type_probe(bytes, probe)?;
    }
    Ok(())
}

fn encode_type_probe(
    bytes: &mut CanonicalBytes,
    probe: &TypeProbe,
) -> Result<(), InjectedProfileError> {
    bytes.u32(probe.identity.0);
    bytes.usize(probe.declaration_count)?;
    bytes.usize(probe.declaration_identities.len())?;
    for (file_ordinal, identity) in &probe.declaration_identities {
        bytes.usize(file_ordinal.index())?;
        bytes.u32(identity.0);
    }
    bytes.usize(probe.member_names.len())?;
    for member in &probe.member_names {
        bytes.string(member)?;
    }
    Ok(())
}

fn encode_value_probe_map(
    bytes: &mut CanonicalBytes,
    probes: &BTreeMap<String, ValueProbe>,
) -> Result<(), InjectedProfileError> {
    bytes.usize(probes.len())?;
    for (name, probe) in probes {
        bytes.string(name)?;
        encode_value_probe(bytes, probe)?;
    }
    Ok(())
}

fn encode_value_probe(
    bytes: &mut CanonicalBytes,
    probe: &ValueProbe,
) -> Result<(), InjectedProfileError> {
    bytes.u32(probe.identity.0);
    bytes.optional_type_id(probe.visible_type);
    bytes.usize(probe.declaration_count)?;
    bytes.usize(probe.participant_identities.len())?;
    for (file_ordinal, identity) in &probe.participant_identities {
        bytes.usize(file_ordinal.index())?;
        bytes.u32(identity.0);
    }
    bytes.usize(probe.call_signature_count)?;
    bytes.usize(probe.member_names.len())?;
    for member in &probe.member_names {
        bytes.string(member)?;
    }
    bytes.usize(probe.callable_members.len())?;
    for member in &probe.callable_members {
        bytes.string(&member.name)?;
        bytes.u32(member.identity.0);
        encode_library_source_unit(bytes, member.source)?;
        encode_library_source_unit(bytes, member.reservation_source)?;
        bytes.u32(member.source_start);
        bytes.usize(member.call_signature_count)?;
        bytes.usize(member.signatures.len())?;
        for signature in &member.signatures {
            bytes.usize(signature.parameter_types.len())?;
            for parameter in &signature.parameter_types {
                bytes.string(parameter)?;
            }
            bytes.string(&signature.return_type)?;
        }
    }
    Ok(())
}

fn encode_library_source_unit(
    bytes: &mut CanonicalBytes,
    source: ExactUnit,
) -> Result<(), InjectedProfileError> {
    let crate::source::SourceUnit::Library { file_ordinal } = source else {
        return Err(InjectedProfileError::CanonicalProjection(
            "injected value publication has a non-library owner".to_owned(),
        ));
    };
    bytes.usize(file_ordinal.index())
}

fn push_frozen_product_section(
    product: &mut Wu0dFrozenLibraryProduct,
    section: Wu0dFrozenProductSection,
    payload: Vec<u8>,
) -> Result<(), InjectedProfileError> {
    product.bytes.push(section.tag());
    let length = u64::try_from(payload.len()).map_err(|_| {
        InjectedProfileError::CanonicalProjection("section length does not fit u64".to_owned())
    })?;
    product.bytes.extend_from_slice(&length.to_be_bytes());
    let start = product.bytes.len();
    product.bytes.extend_from_slice(&payload);
    let end = product.bytes.len();
    product.sections.push(Wu0dFrozenProductSectionDescriptor {
        section,
        range: start..end,
    });
    Ok(())
}

fn parse_frozen_product_section_range(
    product: &[u8],
    requested: Wu0dFrozenProductSection,
) -> Result<Range<usize>, String> {
    const PREFIX: &[u8] = b"typokat-wu0d-frozen-library-product-v2";
    let mut offset = PREFIX.len();
    if !product.starts_with(PREFIX)
        || product.get(offset).copied() != u8::try_from(Wu0dFrozenProductSection::ALL.len()).ok()
    {
        return Err("invalid frozen product header".to_owned());
    }
    offset += 1;
    let mut requested_range = None;
    for expected in Wu0dFrozenProductSection::ALL {
        if product.get(offset).copied() != Some(expected.tag()) {
            return Err("invalid frozen product section order".to_owned());
        }
        offset += 1;
        let length_end = offset
            .checked_add(8)
            .ok_or_else(|| "frozen product section length offset overflow".to_owned())?;
        let length_bytes: [u8; 8] = product
            .get(offset..length_end)
            .ok_or_else(|| "truncated frozen product section length".to_owned())?
            .try_into()
            .map_err(|_| "invalid frozen product section length".to_owned())?;
        offset = length_end;
        let length = usize::try_from(u64::from_be_bytes(length_bytes))
            .map_err(|_| "frozen product section length does not fit usize".to_owned())?;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= product.len())
            .ok_or_else(|| "truncated frozen product section".to_owned())?;
        if expected.tag() == requested.tag() {
            requested_range = Some(offset..end);
        }
        offset = end;
    }
    if offset != product.len() {
        return Err("trailing frozen product bytes".to_owned());
    }
    requested_range.ok_or_else(|| "requested frozen product section is absent".to_owned())
}

fn decode_wu0d_class_terminals_for_test(section: &[u8]) -> Wu0dClassDecodeResult {
    fn read_byte(section: &[u8], offset: &mut usize) -> Result<u8, String> {
        let value = section
            .get(*offset)
            .copied()
            .ok_or_else(|| "truncated WU0D byte".to_owned())?;
        *offset += 1;
        Ok(value)
    }

    fn read_u32(section: &[u8], offset: &mut usize) -> Result<u32, String> {
        let end = (*offset)
            .checked_add(4)
            .ok_or_else(|| "WU0D u32 offset overflow".to_owned())?;
        let value: [u8; 4] = section
            .get(*offset..end)
            .ok_or_else(|| "truncated WU0D u32".to_owned())?
            .try_into()
            .map_err(|_| "invalid WU0D u32".to_owned())?;
        *offset = end;
        Ok(u32::from_be_bytes(value))
    }

    fn read_usize(section: &[u8], offset: &mut usize) -> Result<usize, String> {
        let end = (*offset)
            .checked_add(8)
            .ok_or_else(|| "WU0D u64 offset overflow".to_owned())?;
        let value: [u8; 8] = section
            .get(*offset..end)
            .ok_or_else(|| "truncated WU0D u64".to_owned())?
            .try_into()
            .map_err(|_| "invalid WU0D u64".to_owned())?;
        *offset = end;
        usize::try_from(u64::from_be_bytes(value))
            .map_err(|_| "WU0D u64 does not fit usize".to_owned())
    }

    const PREFIX: &[u8] = b"typokat-wu0d-classes-v1";
    let section = section
        .strip_prefix(PREFIX)
        .ok_or_else(|| "invalid WU0D class section header".to_owned())?;
    let mut offset = 0;
    let count = read_usize(section, &mut offset)?;
    let mut terminals = Vec::with_capacity(count);
    for _ in 0..count {
        let class = crate::types::repr::ClassId(read_u32(section, &mut offset)?);
        let terminal = match read_byte(section, &mut offset)? {
            0 => {
                let type_parameters = read_usize(section, &mut offset)?;
                for _ in 0..type_parameters {
                    read_u32(section, &mut offset)?;
                }
                read_u32(section, &mut offset)?;
                read_u32(section, &mut offset)?;
                match read_byte(section, &mut offset)? {
                    0 => {}
                    1 => {
                        read_u32(section, &mut offset)?;
                    }
                    _ => return Err("invalid WU0D optional class type".to_owned()),
                }
                Wu0dDecodedClassTerminal::Ready
            }
            1 => Wu0dDecodedClassTerminal::HeritagePoison,
            2 => Wu0dDecodedClassTerminal::InitializerPoison,
            3 => Wu0dDecodedClassTerminal::SurfacePoison,
            _ => return Err("invalid WU0D class terminal".to_owned()),
        };
        terminals.push((class, terminal));
    }
    if offset != section.len() {
        return Err("trailing WU0D class terminal bytes".to_owned());
    }
    Ok(terminals)
}

fn canonical_input_for_record(
    canonical: &[CanonicalInput<'_>],
    file_ordinal: LibraryFileOrdinal,
) -> Result<usize, InjectedProfileError> {
    canonical
        .iter()
        .position(|input| input.file_ordinal == file_ordinal)
        .ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "library record has no canonical source".to_owned(),
            )
        })
}

fn encode_library_key(
    bytes: &mut CanonicalBytes,
    key: LibraryEventKey,
) -> Result<(), InjectedProfileError> {
    bytes.usize(key.file_ordinal.index())?;
    bytes.u32(key.source_start);
    bytes.usize(key.event_ordinal)?;
    bytes.usize(key.record_ordinal)?;
    Ok(())
}

fn canonical_record_bytes(
    source: &str,
    record: &CheckerRecord,
) -> Result<Vec<u8>, InjectedProfileError> {
    let mut bytes = CanonicalBytes::domain(b"typokat-wu0d-library-record-v1");
    match record {
        CheckerRecord::Diagnostic(diagnostic) => {
            bytes.byte(0);
            bytes.u32(diagnostic.span.start);
            bytes.u32(diagnostic.span.end);
            let mut rendered = Vec::new();
            render_to_writer_with_format(
                &mut rendered,
                "library",
                source,
                std::slice::from_ref(diagnostic),
                DiagnosticFormat::Compact,
            )
            .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?;
            bytes.bytes(&rendered)?;
        }
        CheckerRecord::Incomplete(incomplete) => {
            bytes.byte(1);
            bytes.string(&incomplete.id)?;
            bytes.u32(incomplete.span.start);
            bytes.u32(incomplete.span.end);
            bytes.string(&incomplete.context)?;
        }
    }
    Ok(bytes.finish())
}

fn canonical_wu0d_semantic_components(
    canonical: &[CanonicalInput<'_>],
    records: &[(LibraryEventKey, CheckerRecord)],
    frozen_library_product: Vec<u8>,
) -> Result<Wu0dSemanticComponents, InjectedProfileError> {
    let mut diagnostics = CanonicalBytes::domain(b"typokat-wu0d-diagnostics-v1");
    let mut incomplete = CanonicalBytes::domain(b"typokat-wu0d-incomplete-v1");
    let mut library_ledger = CanonicalBytes::domain(b"typokat-wu0d-library-ledger-v1");
    let diagnostic_count = records
        .iter()
        .filter(|(_, record)| matches!(record, CheckerRecord::Diagnostic(_)))
        .count();
    let incomplete_count = records.len() - diagnostic_count;
    diagnostics.usize(diagnostic_count)?;
    incomplete.usize(incomplete_count)?;
    library_ledger.usize(records.len())?;
    for (key, record) in records {
        let input = canonical_input_for_record(canonical, key.file_ordinal)?;
        let record_bytes = canonical_record_bytes(canonical[input].source, record)?;
        encode_library_key(&mut library_ledger, *key)?;
        library_ledger.bytes(&record_bytes)?;
        match record {
            CheckerRecord::Diagnostic(_) => {
                encode_library_key(&mut diagnostics, *key)?;
                diagnostics.bytes(&record_bytes)?;
            }
            CheckerRecord::Incomplete(_) => {
                encode_library_key(&mut incomplete, *key)?;
                incomplete.bytes(&record_bytes)?;
            }
        }
    }
    Ok(Wu0dSemanticComponents {
        diagnostics: diagnostics.finish(),
        incomplete: incomplete.finish(),
        library_ledger: library_ledger.finish(),
        frozen_library_product,
    })
}

pub(crate) fn run_injected_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<InjectedProfileRun, InjectedProfileError> {
    let mut boundary = DiagnosticBoundaryAdapter::disabled();
    run_injected_profile_observed(sources, &mut boundary).map(|(run, _)| run)
}

pub(super) fn run_injected_profile_observed(
    sources: &[InjectedLibrarySource<'_>],
    boundary: &mut DiagnosticBoundaryAdapter,
) -> Result<(InjectedProfileRun, String), InjectedProfileError> {
    let parse_started = Instant::now();
    boundary.enter_phase(DiagnosticPhase::Parse, None)?;
    let canonical = canonical_inputs(sources)?;
    let allocators = (0..canonical.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed_and_claims = allocators
        .iter()
        .zip(&canonical)
        .map(|(allocator, input)| {
            let source_type = if input.kind.is_declaration() {
                SourceType::d_ts()
            } else {
                SourceType::ts()
            };
            let parsed = Parser::new(allocator, input.source, source_type).parse();
            if parsed.panicked {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: if parsed.diagnostics.is_empty() {
                        vec!["parser panicked without a diagnostic".to_owned()]
                    } else {
                        parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect()
                    },
                });
            }

            let mut claims = Vec::with_capacity(parsed.diagnostics.len());
            for diagnostic in &parsed.diagnostics {
                let code_is_ts1319 = diagnostic.code.scope.as_deref() == Some("TS")
                    && diagnostic.code.number.as_deref() == Some("1319");
                let [label] = diagnostic.labels.as_slice() else {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                };
                let start = label.offset();
                let Some(end) = start.checked_add(label.len()) else {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                };
                if !code_is_ts1319 {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                }
                claims.push(ParserExportClaim {
                    file_ordinal: input.file_ordinal,
                    span: Span::new(start, end),
                });
            }
            Ok((parsed, claims))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (parsed, claims): (Vec<_>, Vec<_>) = parsed_and_claims.into_iter().unzip();
    let parser_export_claims = claims.into_iter().flatten().collect::<Vec<_>>();
    boundary.exit_phase(DiagnosticPhase::Parse, None)?;
    let parse_elapsed = parse_started.elapsed();

    let bind_started = Instant::now();
    boundary.enter_phase(DiagnosticPhase::Bind, None)?;
    let units = parsed
        .iter()
        .zip(&canonical)
        .map(|(parsed, input)| {
            (
                &parsed.program,
                CompilationUnit {
                    source: input.source_key,
                    origin: CompilationOrigin::Library(input.file_ordinal),
                    binding: ModuleBindingContext::for_program(&parsed.program, input.kind),
                },
            )
        })
        .collect::<Vec<_>>();
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let module_scopes = builder.add_library_modules(&units);
    let binder = builder.finish(module_scopes.last().copied().unwrap_or(ScopeId(0)));
    validate_parser_export_claims(&binder, parser_export_claims, canonical[0].file_ordinal)?;
    let semantic_scopes = units
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|((_, unit), module)| {
            if unit.binding.external_module {
                module
            } else {
                binder.compilation_global
            }
        })
        .collect::<Vec<_>>();
    boundary.exit_phase(DiagnosticPhase::Bind, None)?;
    let bind_elapsed = bind_started.elapsed();

    super::wu0c_attribution::register_wu0c_family_tokens(&binder);
    let reserve_fill_started = Instant::now();
    boundary.enter_phase(DiagnosticPhase::LexicalReservation, None)?;
    let mut ledger = LibraryEventLedger::default();
    let mut lexical_events: LexicalReservations<LibraryRecordTicket> =
        LexicalReservations::default();
    for (input, parsed) in canonical.iter().zip(&parsed) {
        lexical_events
            .reserve_library_program(input.file_ordinal, &parsed.program, &mut ledger)
            .map_err(InjectedProfileError::Reporting)?;
    }
    boundary.exit_phase(DiagnosticPhase::LexicalReservation, None)?;

    boundary.enter_phase(DiagnosticPhase::TypeReservation, None)?;
    let mut interner = Interner::with_intrinsics();
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    let mut type_decls = Vec::new();
    let mut type_resolved = vec![None; binder.type_groups.len()];
    for ((input, parsed), scope) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
    {
        reserve_type_decls(
            &mut interner,
            &binder,
            scope,
            &parsed.program,
            &mut next_type_param,
            &mut next_class_id,
            &mut type_decls,
            &mut type_resolved,
        );
        lexical_events.attach_library_declaration_owners(
            input.file_ordinal,
            &binder,
            scope,
            &parsed.program,
        );
        lexical_events.attach_library_class_bindings(
            input.file_ordinal,
            &binder,
            scope,
            &parsed.program,
            &type_decls,
        );
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;
    boundary.exit_phase(DiagnosticPhase::TypeReservation, None)?;

    boundary.enter_phase(DiagnosticPhase::PassConstruction, None)?;
    let pending_tickets = lexical_events.library_semantic_tickets();
    let mut pass = build_pass_with_tickets(
        &mut interner,
        &binder,
        type_decls,
        type_resolved,
        DeclTypes::new(binder.decl_count),
        next_type_param,
        PassReportingPlan {
            reporting: PassReporting {
                source: library_unit(canonical[0].file_ordinal),
                lexical_events,
                suppress_effects: false,
            },
            pending_tickets,
        },
    );
    boundary.exit_phase(DiagnosticPhase::PassConstruction, None)?;

    let declaration_count = pass.type_decls.len();
    let (fill_scope, fill_start, fill_end) =
        pass.fill_type_decls_range_observed_for_wu0e(binder.module, 0, declaration_count);
    boundary.enter_phase(DiagnosticPhase::FillParameterMetadata, None)?;
    pass.fill_type_group_parameter_metadata_range(fill_start, fill_end);
    boundary.exit_phase(DiagnosticPhase::FillParameterMetadata, None)?;
    boundary.enter_phase(DiagnosticPhase::FillInterfaceScc, None)?;
    pass.construct_pending_interface_sccs_range(fill_start, fill_end);
    boundary.exit_phase(DiagnosticPhase::FillInterfaceScc, None)?;
    boundary.enter_phase(DiagnosticPhase::FillConditional, None)?;
    pass.fill_conditional_aliases_range(fill_start, fill_end);
    boundary.exit_phase(DiagnosticPhase::FillConditional, None)?;
    boundary.enter_phase(DiagnosticPhase::FillMapped, None)?;
    pass.fill_mapped_aliases_range(fill_start, fill_end);
    boundary.exit_phase(DiagnosticPhase::FillMapped, None)?;
    boundary.enter_phase(DiagnosticPhase::FillObject, None)?;
    pass.fill_object_aliases_range(fill_scope, fill_start, fill_end);
    boundary.exit_phase(DiagnosticPhase::FillObject, None)?;
    boundary.enter_phase(DiagnosticPhase::FillRemaining, None)?;
    pass.fill_remaining_aliases_range(fill_scope, fill_start, fill_end);
    boundary.exit_phase(DiagnosticPhase::FillRemaining, None)?;
    let reserve_fill_elapsed = reserve_fill_started.elapsed();

    super::wu0c_attribution::enter_wu0c_phase(
        super::wu0c_attribution::AttributionPhase::PublicationValidation,
    );
    let publication_validation_started = Instant::now();
    let module_programs = module_scopes
        .iter()
        .copied()
        .zip(parsed.iter())
        .map(|(scope, parsed)| (scope, parsed.program.body.as_slice()))
        .collect::<Vec<_>>();
    boundary.enter_phase(DiagnosticPhase::PrepareAttachedNamespaceValues, None)?;
    pass.prepare_project_attached_namespace_values(&module_programs);
    boundary.exit_phase(DiagnosticPhase::PrepareAttachedNamespaceValues, None)?;
    boundary.enter_phase(DiagnosticPhase::PrepareStandaloneNamespaceValues, None)?;
    pass.prepare_project_standalone_namespace_values(&module_programs);
    boundary.exit_phase(DiagnosticPhase::PrepareStandaloneNamespaceValues, None)?;
    boundary.enter_phase(DiagnosticPhase::PublishClassSurfaces, None)?;
    pass.publish_class_surfaces();
    boundary.exit_phase(DiagnosticPhase::PublishClassSurfaces, None)?;
    boundary.enter_phase(DiagnosticPhase::FinalizeStandaloneNamespaceValues, None)?;
    pass.finalize_standalone_namespace_values();
    boundary.exit_phase(DiagnosticPhase::FinalizeStandaloneNamespaceValues, None)?;
    boundary.enter_phase(DiagnosticPhase::PrecomputeStandaloneNamespaceAliases, None)?;
    pass.precompute_standalone_namespace_value_aliases(&module_programs);
    boundary.exit_phase(DiagnosticPhase::PrecomputeStandaloneNamespaceAliases, None)?;
    boundary.enter_phase(DiagnosticPhase::FillPendingInterfaces, None)?;
    pass.fill_pending_interfaces_range(binder.module, 0, declaration_count);
    boundary.exit_phase(DiagnosticPhase::FillPendingInterfaces, None)?;
    boundary.enter_phase(DiagnosticPhase::PublishTypeGroups, None)?;
    let publication_validations = pass.publish_type_groups();
    boundary.exit_phase(DiagnosticPhase::PublishTypeGroups, None)?;
    boundary.enter_phase(DiagnosticPhase::ValidatePublishedClassSurfaces, None)?;
    pass.validate_published_class_surfaces();
    boundary.exit_phase(DiagnosticPhase::ValidatePublishedClassSurfaces, None)?;
    boundary.enter_phase(DiagnosticPhase::CaptureLexicalEvidence, None)?;
    let lexical_source_units = pass
        .lexical_events
        .library_lexical_evidence()
        .iter()
        .copied()
        .collect::<Vec<_>>();
    boundary.exit_phase(DiagnosticPhase::CaptureLexicalEvidence, None)?;
    let publication_validation_elapsed = publication_validation_started.elapsed();

    super::wu0c_attribution::enter_wu0c_phase(
        super::wu0c_attribution::AttributionPhase::StatementCheck,
    );
    let statement_check_started = Instant::now();
    let mut pass_source_units = Vec::with_capacity(canonical.len());
    for (((input, parsed), module), semantic_scope) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(semantic_scopes.iter().copied())
    {
        let statement_ordinal = u32::try_from(input.file_ordinal.index()).map_err(|_| {
            InjectedProfileError::CanonicalProjection(
                "statement file ordinal does not fit u32".to_owned(),
            )
        })?;
        boundary.enter_phase(DiagnosticPhase::StatementFile, Some(statement_ordinal))?;
        pass.current_module = module;
        pass.current_source = library_unit(input.file_ordinal);
        pass_source_units.push(pass.current_source);
        pass.build_flow_graph(semantic_scope, &parsed.program.body);
        pass.check_statements(semantic_scope, &parsed.program.body);
        boundary.exit_phase(DiagnosticPhase::StatementFile, Some(statement_ordinal))?;
    }
    boundary.enter_phase(DiagnosticPhase::SemanticFinalization, None)?;
    let batches = finish_semantic_effects(&mut pass);
    boundary.exit_phase(DiagnosticPhase::SemanticFinalization, None)?;
    boundary.enter_phase(DiagnosticPhase::CollectTypeProbes, None)?;
    let (global_types, module_types) = collect_type_probes(
        &binder,
        pass.type_environment.published(),
        pass.interner.store(),
        &canonical,
        &module_scopes,
    );
    boundary.exit_phase(DiagnosticPhase::CollectTypeProbes, None)?;
    boundary.enter_phase(DiagnosticPhase::CollectGlobalValueProbes, None)?;
    let global_values = collect_value_probes(
        &binder,
        &pass.decl_types,
        pass.interner.store(),
        &pass.namespace_values,
    );
    boundary.exit_phase(DiagnosticPhase::CollectGlobalValueProbes, None)?;
    boundary.enter_phase(DiagnosticPhase::CollectModuleValueProbes, None)?;
    let module_values = collect_module_value_probes(
        &binder,
        &pass.decl_types,
        pass.interner.store(),
        &pass.namespace_values,
        &canonical,
        &module_scopes,
    );
    boundary.exit_phase(DiagnosticPhase::CollectModuleValueProbes, None)?;
    boundary.enter_phase(DiagnosticPhase::CollectNamespaceValueProbes, None)?;
    let namespace_values = collect_namespace_value_probes(
        &binder,
        &pass.decl_types,
        pass.interner.store(),
        &pass.namespace_values,
    );
    boundary.exit_phase(DiagnosticPhase::CollectNamespaceValueProbes, None)?;
    boundary.enter_phase(DiagnosticPhase::FreezeLibraryProduct, None)?;
    let frozen_library_product = canonical_frozen_library_product(Wu0dFrozenLibraryProductInput {
        source_records: &canonical,
        type_publications: pass.type_environment.published(),
        global_types: &global_types,
        module_types: &module_types,
        global_values: &global_values,
        module_values: &module_values,
        namespace_values: &namespace_values,
        classes: pass.type_environment.published().classes(),
        store: pass.interner.store(),
    })?;
    boundary.exit_phase(DiagnosticPhase::FreezeLibraryProduct, None)?;
    boundary.enter_phase(DiagnosticPhase::CompleteSemanticBatches, None)?;
    LibrarySemanticReportingAdapter::new(&mut ledger)
        .complete_semantic_batches(batches)
        .map_err(InjectedProfileError::Reporting)?;
    boundary.exit_phase(DiagnosticPhase::CompleteSemanticBatches, None)?;
    boundary.enter_phase(DiagnosticPhase::ConsumeBinderOutcomes, None)?;
    let reporting_receipts = LibraryReportingConsumer::new(&mut ledger)
        .consume_binder_outcomes(&binder)
        .map_err(InjectedProfileError::Reporting)?;
    boundary.exit_phase(DiagnosticPhase::ConsumeBinderOutcomes, None)?;
    boundary.enter_phase(DiagnosticPhase::FinishLedger, None)?;
    let snapshot = ledger.snapshot();
    let library_records = ledger.finish().map_err(InjectedProfileError::Reporting)?;
    boundary.exit_phase(DiagnosticPhase::FinishLedger, None)?;
    boundary.enter_phase(DiagnosticPhase::BuildSemanticComponents, None)?;
    let wu0d_semantic_components = canonical_wu0d_semantic_components(
        &canonical,
        &library_records,
        frozen_library_product.bytes,
    )?;
    boundary.exit_phase(DiagnosticPhase::BuildSemanticComponents, None)?;
    boundary.enter_phase(DiagnosticPhase::SemanticDigest, None)?;
    let semantic_sha256 =
        canonical_wu0d_semantic_identity(&wu0d_semantic_components).aggregate_sha256;
    boundary.exit_phase(DiagnosticPhase::SemanticDigest, None)?;
    let statement_check_elapsed = statement_check_started.elapsed();

    Ok((
        InjectedProfileRun {
            phase_counts: LibraryPhaseCounts {
                parse_units: parsed.len(),
                bind_units: module_scopes.len(),
                reserved_records: snapshot.reserved_records,
                filled_records: snapshot.filled_records,
                publication_validations,
                statement_check_units: pass_source_units.len(),
            },
            phase_timings: LibraryPhaseTimings {
                parse: parse_elapsed,
                bind: bind_elapsed,
                reserve_fill: reserve_fill_elapsed,
                publication_validation: publication_validation_elapsed,
                statement_check: statement_check_elapsed,
            },
            reserved_file_ordinals: snapshot.reserved_file_ordinals,
            reporting_receipts,
            library_records,
            pass_source_units,
            lexical_source_units,
            wu0d_semantic_components,
            global_types,
            module_types,
            global_values,
        },
        semantic_sha256,
    ))
}

fn validate_parser_export_claims(
    binder: &Binder,
    parser_claims: Vec<ParserExportClaim>,
    fallback_file_ordinal: LibraryFileOrdinal,
) -> Result<(), InjectedProfileError> {
    let mut binder_claims = Vec::new();
    for context in binder.namespaces.export_contexts() {
        if context.syntax != ExportSyntaxDisposition::FutureTk1319 {
            continue;
        }
        let CompilationOrigin::Library(file_ordinal) = context.origin else {
            return Err(InjectedProfileError::Parse {
                file_ordinal: fallback_file_ordinal,
                messages: vec!["binder produced an unowned TK1319 export context".to_owned()],
            });
        };
        if context.kind != ExportContextKind::ExportDefault {
            return Err(InjectedProfileError::Parse {
                file_ordinal,
                messages: vec!["binder produced a non-default TK1319 export context".to_owned()],
            });
        }
        binder_claims.push(ParserExportClaim {
            file_ordinal,
            span: context.span,
        });
    }
    match_parser_export_claims(parser_claims, binder_claims)
}

fn match_parser_export_claims(
    mut parser_claims: Vec<ParserExportClaim>,
    binder_claims: Vec<ParserExportClaim>,
) -> Result<(), InjectedProfileError> {
    for binder_claim in binder_claims {
        let Some(index) = parser_claims
            .iter()
            .position(|parser_claim| *parser_claim == binder_claim)
        else {
            return Err(InjectedProfileError::Parse {
                file_ordinal: binder_claim.file_ordinal,
                messages: vec![format!(
                    "binder TK1319 claim has no parser owner at {}..{}",
                    binder_claim.span.start, binder_claim.span.end
                )],
            });
        };
        parser_claims.remove(index);
    }
    if let Some(parser_claim) = parser_claims.first() {
        return Err(InjectedProfileError::Parse {
            file_ordinal: parser_claim.file_ordinal,
            messages: vec![format!(
                "parser TS1319 claim has no binder owner at {}..{}",
                parser_claim.span.start, parser_claim.span.end
            )],
        });
    }
    Ok(())
}

fn canonical_inputs<'source>(
    sources: &[InjectedLibrarySource<'source>],
) -> Result<Vec<CanonicalInput<'source>>, InjectedProfileError> {
    if sources.is_empty() {
        return Err(InjectedProfileError::EmptyProfile);
    }
    let mut names = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    for source in sources {
        if source.name.is_empty() {
            return Err(InjectedProfileError::EmptyName {
                file_ordinal: source.file_ordinal,
            });
        }
        if !names.insert(source.name) {
            return Err(InjectedProfileError::DuplicateName(source.name.to_owned()));
        }
        if !ordinals.insert(source.file_ordinal) {
            return Err(InjectedProfileError::DuplicateFileOrdinal(
                source.file_ordinal,
            ));
        }
    }
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_by_key(|source| source.file_ordinal);
    sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            let source_key = u32::try_from(index + 1)
                .map(exact_key)
                .map_err(|_| InjectedProfileError::SourceKeyOverflow)?;
            Ok(CanonicalInput {
                file_ordinal: source.file_ordinal,
                name: source.name,
                source: source.source,
                kind: source_file_kind(source.name),
                source_key,
            })
        })
        .collect()
}

fn source_file_kind(name: &str) -> SourceFileKind {
    if name.ends_with(".d.mts") {
        SourceFileKind::DeclarationMts
    } else if name.ends_with(".d.cts") {
        SourceFileKind::DeclarationCts
    } else if name.ends_with(".d.ts") {
        SourceFileKind::DeclarationTs
    } else if name.ends_with(".mts") {
        SourceFileKind::ImplementationMts
    } else if name.ends_with(".cts") {
        SourceFileKind::ImplementationCts
    } else {
        SourceFileKind::ImplementationTs
    }
}

fn collect_type_probes(
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    store: &Store,
    canonical: &[CanonicalInput<'_>],
    module_scopes: &[ScopeId],
) -> (
    BTreeMap<String, TypeProbe>,
    BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
) {
    let mut globals = BTreeMap::new();
    let mut modules = BTreeMap::new();
    for group in binder.type_groups.iter() {
        let probe = type_probe(binder, published, store, group.id);
        let global_symbol = binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&group.name))
            .and_then(|symbol| binder.symbols.get(symbol));
        if global_symbol.is_some_and(|symbol| symbol.ty == Some(group.id)) {
            globals.insert(group.name.clone(), probe.clone());
        }
        for (input, scope) in canonical.iter().zip(module_scopes) {
            let local_symbol = binder
                .graph
                .get(*scope)
                .and_then(|scope| scope.lookup_local(&group.name))
                .and_then(|symbol| binder.symbols.get(symbol));
            if local_symbol.is_some_and(|symbol| symbol.ty == Some(group.id)) {
                modules.insert((input.file_ordinal, group.name.clone()), probe.clone());
            }
        }
    }
    (globals, modules)
}

fn type_probe(
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    store: &Store,
    identity: TypeGroupId,
) -> TypeProbe {
    let group = binder
        .type_groups
        .get(identity)
        .expect("published type probe has a binder group");
    let declaration_identities = group
        .fragments
        .iter()
        .filter_map(|fragment| {
            library_ordinal(
                binder
                    .namespaces
                    .compilation_origin_for_source(fragment.source)?,
            )
            .map(|file_ordinal| (file_ordinal, identity))
        })
        .collect::<Vec<_>>();
    let member_names = published
        .groups()
        .get(identity)
        .and_then(|terminal| match terminal {
            PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                PublishedTypeGroupSurface::Template(ty) => Some(ty),
                PublishedTypeGroupSurface::Class(class) => {
                    match published.classes().published_class(class) {
                        DemandOutcome::Ready(surface) => Some(surface.instance_template()),
                        DemandOutcome::Exhausted(_) => None,
                    }
                }
            },
            PublishedTypeGroupTerminal::Unavailable(_) => None,
        })
        .and_then(|ty| store.object_type(ty))
        .map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.name.clone())
                .collect()
        })
        .unwrap_or_default();
    TypeProbe {
        identity,
        declaration_identities,
        declaration_count: group.fragments.len(),
        member_names,
    }
}

fn collect_value_probes(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<LibraryRecordTicket>,
) -> BTreeMap<String, ValueProbe> {
    let mut probes = BTreeMap::new();
    for (symbol_id, symbol) in binder.symbols.iter() {
        if binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&symbol.name))
            != Some(symbol_id)
        {
            continue;
        }
        if let Some(probe) = value_probe_for_symbol(
            binder,
            decl_types,
            store,
            namespace_values,
            binder.compilation_global,
            symbol_id,
        ) {
            probes.insert(symbol.name.clone(), probe);
        }
    }
    probes
}

fn collect_module_value_probes(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<LibraryRecordTicket>,
    canonical: &[CanonicalInput<'_>],
    module_scopes: &[ScopeId],
) -> BTreeMap<(LibraryFileOrdinal, String), ValueProbe> {
    let mut probes = BTreeMap::new();
    for (input, owner_scope) in canonical.iter().zip(module_scopes) {
        for (symbol_id, symbol) in binder.symbols.iter() {
            if binder
                .graph
                .get(*owner_scope)
                .and_then(|scope| scope.lookup_local(&symbol.name))
                != Some(symbol_id)
            {
                continue;
            }
            if let Some(probe) = value_probe_for_symbol(
                binder,
                decl_types,
                store,
                namespace_values,
                *owner_scope,
                symbol_id,
            ) {
                probes.insert((input.file_ordinal, symbol.name.clone()), probe);
            }
        }
    }
    probes
}

fn collect_namespace_value_probes(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<LibraryRecordTicket>,
) -> Vec<NamespaceValueProbe> {
    let mut probes = binder
        .standalone_namespace_value_attachments()
        .into_iter()
        .filter_map(|attachment| {
            let symbol = binder.symbols.get(attachment.symbol)?;
            let owner_scope = attachment
                .fragments
                .first()
                .map_or(binder.compilation_global, |fragment| fragment.module);
            let value = value_probe_for_symbol(
                binder,
                decl_types,
                store,
                namespace_values,
                owner_scope,
                attachment.symbol,
            )?;
            Some(NamespaceValueProbe {
                namespace: attachment.namespace.0,
                name: symbol.name.clone(),
                value,
            })
        })
        .collect::<Vec<_>>();
    probes.sort_by_key(|probe| probe.namespace);
    probes
}

fn value_probe_for_symbol(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<LibraryRecordTicket>,
    owner_scope: ScopeId,
    symbol_id: SymbolId,
) -> Option<ValueProbe> {
    let symbol = binder.symbols.get(symbol_id)?;
    let identity = symbol.value?;
    let participant_identities = symbol
        .declarations
        .iter()
        .filter_map(|declaration| {
            let declaration = binder.declarations.get(*declaration)?;
            origin_for_module(binder, declaration.site.module)
                .map(|file_ordinal| (file_ordinal, identity))
        })
        .collect::<Vec<_>>();
    let visible = decl_types.get(identity);
    let call_signature_count = visible
        .map(|ty| signature_ids(store, ty).len())
        .unwrap_or_default();
    let member_names = visible
        .and_then(|ty| store.object_type(ty))
        .map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let callable_members = binder
        .namespace_value_attachment(owner_scope, &symbol.name)
        .map(|attachment| {
            attachment
                .members
                .into_iter()
                .filter(|member| member.kind == MergeDeclarationKind::Function)
                .filter_map(|member| {
                    let member_identity = member.value_storage?;
                    let file_ordinal = library_ordinal(member.origin)?;
                    let reservation =
                        namespace_values.namespace_function_reservation(member.declaration)?;
                    let property_ty = visible
                        .and_then(|ty| store.object_type(ty))
                        .and_then(|object| object.property(member.name))
                        .map(|property| property.ty)?;
                    let signature_ids = signature_ids(store, property_ty);
                    let signatures = signature_ids
                        .iter()
                        .filter_map(|signature| signature_probe(store, *signature))
                        .collect::<Vec<_>>();
                    Some(CallableMemberProbe {
                        name: member.name.to_owned(),
                        identity: member_identity,
                        source: library_unit(file_ordinal),
                        reservation_source: reservation.unit,
                        source_start: member.site.declaration_span.start,
                        call_signature_count: signature_ids.len(),
                        signatures,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(ValueProbe {
        identity,
        visible_type: visible,
        participant_identities,
        declaration_count: symbol.declarations.len(),
        call_signature_count,
        member_names,
        callable_members,
    })
}

fn signature_ids(store: &Store, ty: TypeId) -> Vec<TypeId> {
    match store.tag(ty) {
        TypeTag::Function => vec![ty],
        TypeTag::Object => store
            .object_type(ty)
            .map(|object| object.call_signatures.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn signature_probe(store: &Store, ty: TypeId) -> Option<SignatureProbe> {
    let signature = store.function_type(ty)?;
    Some(SignatureProbe {
        parameter_types: signature
            .params
            .iter()
            .map(|parameter| render_type(store, parameter.ty, false))
            .collect(),
        return_type: render_type(store, signature.ret, false),
    })
}

fn origin_for_module(binder: &Binder, module: ScopeId) -> Option<LibraryFileOrdinal> {
    binder
        .namespaces
        .source_units()
        .find(|unit| unit.module == module)
        .and_then(|unit| library_ordinal(unit.origin))
}

fn library_ordinal(origin: CompilationOrigin) -> Option<LibraryFileOrdinal> {
    match origin {
        CompilationOrigin::Library(file_ordinal) => Some(file_ordinal),
        CompilationOrigin::User(_) => None,
    }
}

fn release_phase_line(
    process: usize,
    registry_validation: Duration,
    timings: &LibraryPhaseTimings,
    total: Duration,
) -> String {
    format!(
        "typokat-wu0b-phase-v1 process={process} registry_validation_us={} parse_us={} bind_us={} reserve_fill_us={} publication_validation_us={} statement_check_us={} total_us={}",
        registry_validation.as_micros(),
        timings.parse.as_micros(),
        timings.bind.as_micros(),
        timings.reserve_fill.as_micros(),
        timings.publication_validation.as_micros(),
        timings.statement_check.as_micros(),
        total.as_micros(),
    )
}

struct ReleaseOutcomeLine {
    process: usize,
    file_count: usize,
    reserved_records: usize,
    filled_records: usize,
    publication_validations: usize,
    library_diagnostics: usize,
    library_incompletes: usize,
    tiny_parse_errors: usize,
    tiny_diagnostics: usize,
    tiny_incompletes: usize,
}

impl ReleaseOutcomeLine {
    fn render(&self) -> String {
        format!(
            "typokat-wu0b-outcome-v1 process={} file_count={} reserved_records={} filled_records={} publication_validations={} library_diagnostics={} library_incompletes={} tiny_parse_errors={} tiny_diagnostics={} tiny_incompletes={}",
            self.process,
            self.file_count,
            self.reserved_records,
            self.filled_records,
            self.publication_validations,
            self.library_diagnostics,
            self.library_incompletes,
            self.tiny_parse_errors,
            self.tiny_diagnostics,
            self.tiny_incompletes,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::checker::wu0b_profile::load_strict_profile;
    use crate::driver::check_source;

    fn assert_owned_terminal<T: Send + Sync + 'static>() {}

    const TINY_SOURCE: &str = "export const typokatWu0bProbe: number = 1;\n";

    #[test]
    fn injected_results_are_ast_free_owned_terminals() {
        assert_owned_terminal::<InjectedProfileRun>();
        assert_owned_terminal::<InjectedProfileError>();
        assert_owned_terminal::<LibraryPhaseTimings>();
    }

    #[test]
    fn phase_timings_are_real_nonoverlapping_measurements() {
        let started = Instant::now();
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "timing.d.ts",
            source: "interface TimingWitness { value: number; }",
        }])
        .expect("focused timing profile");
        let external = started.elapsed();
        assert!(run.phase_timings.measured_total() <= external);
        assert_eq!(run.phase_counts.parse_units, 1);
        assert_eq!(run.phase_counts.bind_units, 1);
        assert_eq!(run.phase_counts.statement_check_units, 1);
    }

    #[test]
    fn release_probe_lines_have_the_exact_v1_contract() {
        let timings = LibraryPhaseTimings {
            parse: Duration::from_micros(2),
            bind: Duration::from_micros(3),
            reserve_fill: Duration::from_micros(5),
            publication_validation: Duration::from_micros(7),
            statement_check: Duration::from_micros(11),
        };
        assert_eq!(
            release_phase_line(
                4,
                Duration::from_micros(1),
                &timings,
                Duration::from_micros(31),
            ),
            "typokat-wu0b-phase-v1 process=4 registry_validation_us=1 parse_us=2 bind_us=3 reserve_fill_us=5 publication_validation_us=7 statement_check_us=11 total_us=31"
        );
        assert_eq!(
            ReleaseOutcomeLine {
                process: 4,
                file_count: 82,
                reserved_records: 13,
                filled_records: 13,
                publication_validations: 9,
                library_diagnostics: 2,
                library_incompletes: 3,
                tiny_parse_errors: 0,
                tiny_diagnostics: 0,
                tiny_incompletes: 0,
            }
            .render(),
            "typokat-wu0b-outcome-v1 process=4 file_count=82 reserved_records=13 filled_records=13 publication_validations=9 library_diagnostics=2 library_incompletes=3 tiny_parse_errors=0 tiny_diagnostics=0 tiny_incompletes=0"
        );
    }

    #[test]
    #[ignore = "release-only WU0B cold-process measurement"]
    fn wu0b_release_probe_once() {
        let process = std::env::var("TYPOKAT_WU0B_PROCESS")
            .expect("TYPOKAT_WU0B_PROCESS must identify release process 1..5")
            .parse::<usize>()
            .expect("TYPOKAT_WU0B_PROCESS must be an integer in 1..5");
        assert!(
            (1..=5).contains(&process),
            "TYPOKAT_WU0B_PROCESS must be in 1..5"
        );

        let total_started = Instant::now();
        let registry_started = Instant::now();
        let profile = load_strict_profile().expect("strict WU0B registry validation");
        let registry_validation = registry_started.elapsed();
        let injected = profile.injected_sources();
        let _wu0c_attribution = super::super::wu0c_attribution::start_wu0c_attribution_from_env();
        let run = run_injected_profile(&injected).expect("exact WU0B profile execution");

        assert_eq!(run.phase_counts.parse_units, 82);
        assert_eq!(run.phase_counts.bind_units, 82);
        assert_eq!(run.phase_counts.statement_check_units, 82);
        assert!(run.phase_counts.reserved_records > 0);
        assert_eq!(
            run.phase_counts.reserved_records,
            run.phase_counts.filled_records
        );
        assert!(run.phase_counts.publication_validations > 0);
        let expected = (0..82)
            .map(LibraryFileOrdinal::new)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            run.reserved_file_ordinals
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            expected
        );
        assert!(run
            .library_records
            .iter()
            .all(|(key, _)| expected.contains(&key.file_ordinal)));
        assert!(run
            .reporting_receipts
            .iter()
            .all(|receipt| expected.contains(&receipt.file_ordinal)));

        let tiny = check_source(TINY_SOURCE);
        assert!(tiny.parse_errors.is_empty(), "{:?}", tiny.parse_errors);
        assert!(tiny.diagnostics.is_empty(), "{:?}", tiny.diagnostics);
        assert!(tiny.incomplete.is_empty(), "{:?}", tiny.incomplete);
        let library_diagnostics = run
            .library_records
            .iter()
            .filter(|(_, record)| matches!(record, CheckerRecord::Diagnostic(_)))
            .count();
        let library_incompletes = run.library_records.len() - library_diagnostics;
        let total = total_started.elapsed();
        assert!(
            registry_validation + run.phase_timings.measured_total() <= total,
            "external total must cover every measured phase"
        );

        println!(
            "{}",
            release_phase_line(process, registry_validation, &run.phase_timings, total)
        );
        println!(
            "{}",
            ReleaseOutcomeLine {
                process,
                file_count: injected.len(),
                reserved_records: run.phase_counts.reserved_records,
                filled_records: run.phase_counts.filled_records,
                publication_validations: run.phase_counts.publication_validations,
                library_diagnostics,
                library_incompletes,
                tiny_parse_errors: tiny.parse_errors.len(),
                tiny_diagnostics: tiny.diagnostics.len(),
                tiny_incompletes: tiny.incomplete.len(),
            }
            .render()
        );
    }

    #[test]
    fn recoverable_parser_diagnostic_fails_closed_before_profile_execution() {
        let file_ordinal = LibraryFileOrdinal::new(3);
        let source = "declare namespace Broken { export = Broken; }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "witness must be recoverable");
        assert_eq!(parsed.diagnostics.len(), 1, "witness must diagnose once");
        assert_eq!(parsed.diagnostics[0].code.scope.as_deref(), Some("TS"));
        assert_eq!(parsed.diagnostics[0].code.number.as_deref(), Some("1063"));
        let result = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "recoverable.ts",
            source,
        }]);
        let Err(InjectedProfileError::Parse {
            file_ordinal: actual,
            messages,
        }) = result
        else {
            panic!("recoverable parser diagnostics must abort the injected run");
        };
        assert_eq!(actual, file_ordinal);
        assert!(!messages.is_empty());
    }

    #[test]
    fn focused_shared_interface_identity_and_surface() {
        let run = run_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(1),
                name: "first.d.ts",
                source: "interface Shared { first: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(2),
                name: "second.d.ts",
                source: "interface Shared { second: string; }",
            },
        ])
        .expect("focused injected profile");
        let shared = run.global_type_probe("Shared").expect("shared type probe");
        assert_eq!(shared.declaration_count, 2);
        assert_eq!(shared.member_names, ["first", "second"]);
        assert_eq!(run.phase_counts.parse_units, 2);
        assert_eq!(run.phase_counts.bind_units, 2);
        assert_eq!(run.phase_counts.statement_check_units, 2);
        assert_eq!(
            run.phase_counts.reserved_records,
            run.phase_counts.filled_records
        );
        assert_eq!(run.phase_counts.publication_validations, 1);
        assert_eq!(
            run.reserved_file_ordinals,
            [LibraryFileOrdinal::new(1), LibraryFileOrdinal::new(2)]
        );
        assert!(run.reporting_receipts.is_empty());
        assert!(run.library_records.is_empty());
    }

    #[test]
    fn focused_implementation_diagnostic_keeps_exact_library_key() {
        let file_ordinal = LibraryFileOrdinal::new(7);
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "broken.ts",
            source: "const broken: number = 'wrong';",
        }])
        .expect("focused injected profile");
        assert_eq!(run.pass_source_units, [library_unit(file_ordinal)]);
        assert!(!run.lexical_source_units.is_empty());
        assert_eq!(run.library_records.len(), 1);
        assert_eq!(run.library_records[0].0.file_ordinal, file_ordinal);
        let CheckerRecord::Diagnostic(diagnostic) = &run.library_records[0].1 else {
            panic!("implementation mismatch must be a diagnostic");
        };
        assert_eq!(run.library_records[0].0.source_start, diagnostic.span.start);
    }

    #[test]
    fn focused_function_namespace_and_module_private_probes() {
        let script = LibraryFileOrdinal::new(10);
        let module = LibraryFileOrdinal::new(11);
        let run = run_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: script,
                name: "function.d.ts",
                source: "declare function Merged(value: number): string; declare namespace Merged { export function member(value: string): number; } interface Shared { script: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: module,
                name: "module.d.ts",
                source: "export {}; interface Private { local: boolean; } declare global { interface Shared { module: string; } }",
            },
        ])
        .expect("focused injected profile");

        let merged = run.global_value_probe("Merged").expect("merged value");
        assert_eq!(merged.declaration_count, 2);
        assert_eq!(merged.call_signature_count, 1, "{merged:?}");
        assert_eq!(merged.member_names, ["member"], "{merged:?}");
        assert_eq!(merged.callable_members.len(), 1);
        assert_eq!(merged.callable_members[0].signatures.len(), 1);
        assert!(run.global_type_probe("Private").is_none());
        assert!(run.module_type_probe(module, "Private").is_some());
        assert_eq!(
            run.global_type_probe("Shared")
                .expect("augmented global")
                .declaration_count,
            2
        );
    }

    #[test]
    fn focused_typed_ts1319_claim_is_reported_once_by_binder_consumer() {
        let file_ordinal = LibraryFileOrdinal::new(14);
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "export-context.d.ts",
            source: "declare namespace Exported { export default function f(): void; }",
        }])
        .expect("typed TS1319 claim must transfer to binder reporting");
        let receipt = run
            .reporting_receipts
            .iter()
            .find(|receipt| receipt.family == LibraryReportingFamily::ExportContext)
            .expect("export-context receipt");
        assert_eq!(receipt.file_ordinal, file_ordinal);
        assert_eq!(receipt.observed_outcomes, 1);
        assert_eq!(receipt.emitted_records, 1);
        assert_eq!(run.library_records.len(), 1);
        let (key, record) = &run.library_records[0];
        assert_eq!(key.file_ordinal, file_ordinal);
        assert_eq!(key.source_start, 29);
        let CheckerRecord::Incomplete(incomplete) = record else {
            panic!("TK1319 reporting must remain an incomplete record");
        };
        assert_eq!(incomplete.id, "library/export-context/future-tk1319");
        assert_eq!(
            incomplete.context,
            "library export-context TK1319 reporting is deferred beyond WU0B"
        );
        assert_eq!(incomplete.span, Span::new(29, 63));
        assert_eq!(key.source_start, incomplete.span.start);
    }

    #[test]
    fn mixed_ts1319_and_ts1063_diagnostics_fail_closed() {
        let file_ordinal = LibraryFileOrdinal::new(15);
        let source =
            "declare namespace Mixed { export default function f(): void; export = Mixed; }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "witness must be recoverable");
        let codes = parsed
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code.scope.as_deref(),
                    diagnostic.code.number.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert!(codes.contains(&(Some("TS"), Some("1319"))), "{codes:?}");
        assert!(codes.contains(&(Some("TS"), Some("1063"))), "{codes:?}");
        let result = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "mixed.ts",
            source,
        }]);
        assert!(matches!(
            result,
            Err(InjectedProfileError::Parse {
                file_ordinal: actual,
                ..
            }) if actual == file_ordinal
        ));
    }

    #[test]
    fn parser_export_claim_inventory_rejects_duplicates_and_unmatched_binder_claims() {
        let claim = ParserExportClaim {
            file_ordinal: LibraryFileOrdinal::new(16),
            span: Span::new(4, 12),
        };
        assert!(matches!(
            match_parser_export_claims(vec![claim, claim], vec![claim]),
            Err(InjectedProfileError::Parse { .. })
        ));
        assert!(matches!(
            match_parser_export_claims(Vec::new(), vec![claim]),
            Err(InjectedProfileError::Parse { .. })
        ));
    }

    #[test]
    fn focused_identical_callable_offsets_keep_exact_owners_and_signatures() {
        let first = LibraryFileOrdinal::new(34);
        let second = LibraryFileOrdinal::new(35);
        let sources = [
            InjectedLibrarySource {
                file_ordinal: first,
                name: "first.d.ts",
                source: "declare namespace OffsetCallable { export function alpha(value: number): string; }\ndeclare function OffsetCallable(): void;",
            },
            InjectedLibrarySource {
                file_ordinal: second,
                name: "second.d.ts",
                source: "declare namespace OffsetCallable { export function bravo(value: string): number; }",
            },
        ];
        let reversed = [sources[1], sources[0]];

        for run in [
            run_injected_profile(&sources).expect("forward injected profile"),
            run_injected_profile(&reversed).expect("reverse injected profile"),
        ] {
            let merged = run
                .global_value_probe("OffsetCallable")
                .expect("merged callable");
            let mut members = merged.callable_members.clone();
            members.sort_by(|left, right| left.name.cmp(&right.name));
            assert_eq!(members.len(), 2);
            assert_ne!(members[0].identity, members[1].identity);
            for (member, name, file, parameter, result) in [
                (&members[0], "alpha", first, "number", "string"),
                (&members[1], "bravo", second, "string", "number"),
            ] {
                assert_eq!(member.name, name);
                assert_eq!(member.source, library_unit(file));
                assert_eq!(member.reservation_source, library_unit(file));
                assert_eq!(member.source_start, 42);
                assert_eq!(member.call_signature_count, 1);
                assert_eq!(member.signatures.len(), 1);
                assert_eq!(member.signatures[0].parameter_types, [parameter]);
                assert_eq!(member.signatures[0].return_type, result);
            }
        }
    }

    #[test]
    fn focused_function_namespace_merges_are_input_order_independent() {
        let sources = [
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(30),
                name: "function.d.ts",
                source: "declare function FunctionFirst(value: number): string;",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(31),
                name: "function-namespace.d.ts",
                source: "declare namespace FunctionFirst { export const tag: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(32),
                name: "namespace.d.ts",
                source: "declare namespace NamespaceFirst { export const tag: string; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(33),
                name: "namespace-function.d.ts",
                source: "declare function NamespaceFirst(value: string): number;",
            },
        ];
        let reversed = [sources[3], sources[2], sources[1], sources[0]];
        for run in [
            run_injected_profile(&sources).expect("forward injected profile"),
            run_injected_profile(&reversed).expect("reverse injected profile"),
        ] {
            for name in ["FunctionFirst", "NamespaceFirst"] {
                let merged = run.global_value_probe(name).expect("merged value");
                assert_eq!(merged.declaration_count, 2);
                assert_eq!(merged.call_signature_count, 1);
                assert_eq!(merged.member_names, ["tag"]);
            }
            assert!(run.library_records.is_empty());
        }
    }

    #[test]
    fn focused_parse_clean_binder_reporting_families_have_real_receipts() {
        for (index, name, source, family) in [
            (
                10,
                "alias.d.ts",
                "declare namespace AliasOutput { interface Local {} export { A as B }; export { Local as A }; }",
                LibraryReportingFamily::LocalAmbientExportAliasFailure,
            ),
            (
                11,
                "placement.ts",
                "namespace Late { export const value = 1; } function Late(): void {}",
                LibraryReportingFamily::PlacementIssue,
            ),
            (
                12,
                "global.d.ts",
                "declare global { interface InvalidScriptGlobal {} }",
                LibraryReportingFamily::GlobalAugmentation,
            ),
            (
                13,
                "umd.d.ts",
                "export as namespace ScriptUmd;",
                LibraryReportingFamily::UmdExportContext,
            ),
            (
                15,
                "member.d.ts",
                "declare namespace MemberRoot { const value: number; }",
                LibraryReportingFamily::NamespaceMember,
            ),
            (
                16,
                "standalone.d.ts",
                "declare namespace StandaloneRoot { const value: number; }",
                LibraryReportingFamily::StandaloneNamespaceValueMember,
            ),
        ] {
            let file_ordinal = LibraryFileOrdinal::new(index);
            let run = run_injected_profile(&[InjectedLibrarySource {
                file_ordinal,
                name,
                source,
            }])
            .expect("binder reporting profile");
            let receipt = run
                .reporting_receipts
                .iter()
                .find(|receipt| receipt.family == family)
                .expect("family receipt");
            assert_eq!(receipt.file_ordinal, file_ordinal);
            assert_eq!(receipt.observed_outcomes, 1);
        }
    }
}
