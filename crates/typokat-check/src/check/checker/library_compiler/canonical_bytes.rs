use super::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct ParserExportClaim {
    pub(super) file_ordinal: LibraryFileOrdinal,
    pub(super) span: Span,
}

pub(super) struct CanonicalBytes(Vec<u8>);

impl CanonicalBytes {
    pub(super) fn domain(domain: &[u8]) -> Self {
        Self(Vec::from(domain))
    }

    fn byte(&mut self, value: u8) {
        self.0.push(value);
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn bool(&mut self, value: bool) {
        self.byte(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    pub(super) fn usize(&mut self, value: usize) -> Result<(), InjectedProfileError> {
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

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn property_key(&mut self, key: &PropertyKey) -> Result<(), InjectedProfileError> {
        match key {
            PropertyKey::String(name) => {
                self.byte(0);
                self.string(name)
            }
            PropertyKey::WellKnownSymbol(symbol) => {
                self.byte(1);
                self.byte(well_known_symbol_code(*symbol));
                Ok(())
            }
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn type_id(&mut self, value: TypeId) {
        self.u32(value.0);
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn optional_type_id(&mut self, value: Option<TypeId>) {
        self.bool(value.is_some());
        if let Some(value) = value {
            self.type_id(value);
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn type_ids(&mut self, values: &[TypeId]) -> Result<(), InjectedProfileError> {
        self.usize(values.len())?;
        for value in values {
            self.type_id(*value);
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(any(test, feature = "test-utils"))]
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
        IntrinsicKind::Object => 16,
        IntrinsicKind::BigInt => 17,
        IntrinsicKind::Symbol => 18,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn visibility_code(visibility: Visibility) -> u8 {
    match visibility {
        Visibility::Public => 0,
        Visibility::Private => 1,
        Visibility::Protected => 2,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn well_known_symbol_code(symbol: WellKnownSymbol) -> u8 {
    match symbol {
        WellKnownSymbol::Iterator => 0,
        WellKnownSymbol::ToStringTag => 1,
        WellKnownSymbol::AsyncIterator => 2,
        WellKnownSymbol::Species => 3,
        WellKnownSymbol::ToPrimitive => 4,
        WellKnownSymbol::Replace => 5,
        WellKnownSymbol::Unscopables => 6,
        WellKnownSymbol::Split => 7,
        WellKnownSymbol::Search => 8,
        WellKnownSymbol::Match => 9,
        WellKnownSymbol::MatchAll => 10,
        WellKnownSymbol::HasInstance => 11,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn modifier_code(modifier: ModifierOp) -> u8 {
    match modifier {
        ModifierOp::Keep => 0,
        ModifierOp::Add => 1,
        ModifierOp::Remove => 2,
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn encode_store_row(
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
            LiteralValue::WellKnownSymbol(symbol) => {
                bytes.byte(3);
                bytes.byte(well_known_symbol_code(*symbol));
            }
        },
        TypeTag::Enum => {
            let enum_type = store.enum_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing enum payload".to_owned())
            })?;
            bytes.u32(enum_type.id.0);
            bytes.string(&enum_type.name)?;
            bytes.bool(enum_type.numeric_open);
            bytes.usize(enum_type.members.len())?;
            for member in &enum_type.members {
                bytes.u32(member.id.0);
                bytes.string(&member.name)?;
                encode_enum_value(bytes, &member.value)?;
            }
        }
        TypeTag::EnumMember => {
            let member = store.enum_member_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing enum-member payload".to_owned())
            })?;
            bytes.u32(member.enum_id.0);
            bytes.u32(member.member_id.0);
            bytes.string(&member.enum_name)?;
            bytes.string(&member.member_name)?;
            encode_enum_value(bytes, &member.value)?;
            bytes.bool(member.numeric_open);
        }
        TypeTag::Object => {
            let object = store.object_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing object payload".to_owned())
            })?;
            bytes.usize(object.properties.len())?;
            for property in &object.properties {
                bytes.property_key(&property.key)?;
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
        TypeTag::Declared => {
            let declared = store.declared_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing declared payload".to_owned())
            })?;
            bytes.u32(declared.recipe.0);
            bytes.usize(declared.mapper.len())?;
            for (parameter, value) in &declared.mapper {
                bytes.u32(parameter.0);
                bytes.type_id(*value);
            }
        }
    }
    bytes.bool(store.template_name(id).is_some());
    if let Some(name) = store.template_name(id) {
        bytes.string(name)?;
    }
    Ok(())
}

#[cfg(any(test, feature = "test-utils"))]
fn encode_enum_value(
    bytes: &mut CanonicalBytes,
    value: &EnumValue,
) -> Result<(), InjectedProfileError> {
    match value {
        EnumValue::Number(value) => {
            bytes.byte(0);
            bytes.u64(value.canonical_bits());
        }
        EnumValue::String(value) => {
            bytes.byte(1);
            bytes.string(value)?;
        }
        EnumValue::OpaqueNumber => bytes.byte(2),
    }
    Ok(())
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn encode_declared_recipe_row(
    bytes: &mut CanonicalBytes,
    store: &Store,
    id: crate::types::repr::DeclaredRecipeId,
) -> Result<(), InjectedProfileError> {
    use crate::types::repr::DeclaredRecipeNode;
    let recipe = store.declared_recipe(id).ok_or_else(|| {
        InjectedProfileError::CanonicalProjection("missing declared recipe".to_owned())
    })?;
    match &recipe.node {
        DeclaredRecipeNode::Type(ty) => {
            bytes.byte(0);
            bytes.type_id(*ty);
        }
        DeclaredRecipeNode::Array(element) => {
            bytes.byte(1);
            bytes.u32(element.0);
        }
        DeclaredRecipeNode::Tuple { elements, rest } => {
            bytes.byte(2);
            bytes.usize(elements.len())?;
            for element in elements {
                bytes.u32(element.0);
            }
            bytes.bool(rest.is_some());
            if let Some((position, rest)) = rest {
                bytes.usize(*position)?;
                bytes.u32(rest.0);
            }
        }
        DeclaredRecipeNode::Readonly(operand) => {
            bytes.byte(3);
            bytes.u32(operand.0);
        }
        DeclaredRecipeNode::Application {
            template,
            parameters,
            arguments,
        } => {
            bytes.byte(4);
            bytes.type_id(*template);
            bytes.usize(parameters.len())?;
            for parameter in parameters {
                bytes.u32(parameter.0);
            }
            bytes.usize(arguments.len())?;
            for argument in arguments {
                bytes.u32(argument.0);
            }
        }
    }
    bytes.usize(recipe.free_params.len())?;
    for parameter in &recipe.free_params {
        bytes.u32(parameter.0);
    }
    Ok(())
}

pub(super) fn canonical_input_for_record(
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

#[cfg(any(test, feature = "test-utils"))]
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

pub(super) fn canonical_record_bytes(
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
            record_collision_plan_forbidden_work(|work| {
                work.rendered_record_digest_bytes = work
                    .rendered_record_digest_bytes
                    .saturating_add(u64::try_from(rendered.len()).unwrap_or(u64::MAX));
            });
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

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn full_structured_record_baseline(
    owner: ReplayOwner,
    records: &[&CheckerRecord],
) -> super::replay_index::ReplayBaselineRecord {
    fn bytes(hasher: &mut Sha256, value: &[u8]) {
        hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(value);
    }

    fn span(hasher: &mut Sha256, span: Span) {
        hasher.update(span.start.to_be_bytes());
        hasher.update(span.end.to_be_bytes());
    }

    let mut hasher = Sha256::new();
    hasher.update(b"typokat/collision-record-fingerprint/v1");
    hasher.update(
        u64::try_from(records.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for record in records {
        match record {
            CheckerRecord::Diagnostic(diagnostic) => {
                hasher.update([0]);
                bytes(&mut hasher, diagnostic.code.as_str().as_bytes());
                hasher.update([match diagnostic.severity {
                    crate::diagnostics::Severity::Error => 0,
                    crate::diagnostics::Severity::Warning => 1,
                }]);
                bytes(&mut hasher, diagnostic.message.as_bytes());
                span(&mut hasher, diagnostic.span);
                hasher.update(
                    u64::try_from(diagnostic.elaboration().len())
                        .unwrap_or(u64::MAX)
                        .to_be_bytes(),
                );
                for line in diagnostic.elaboration() {
                    bytes(&mut hasher, line.as_bytes());
                }
            }
            CheckerRecord::Incomplete(incomplete) => {
                hasher.update([1]);
                bytes(&mut hasher, incomplete.id.as_bytes());
                span(&mut hasher, incomplete.span);
                bytes(&mut hasher, incomplete.context.as_bytes());
            }
        }
    }
    super::replay_index::ReplayBaselineRecord {
        owner,
        record_count: u64::try_from(records.len()).unwrap_or(u64::MAX),
        digest: hasher.finalize().into(),
    }
}

/// Project the library's records onto their canonical byte blobs.
///
/// Test-only by construction: the blobs pinned the retired artifact's sections and no compile
/// reads them, so the projection is a suite assertion rather than startup work (ADR-0017).
#[cfg(any(test, feature = "test-utils"))]
pub fn canonical_library_evidence_for_test(
    sources: &[InjectedLibrarySource<'_>],
    records: &[(LibraryEventKey, CheckerRecord)],
) -> Result<CanonicalLibraryEvidence, InjectedProfileError> {
    let mut diagnostics = CanonicalBytes::domain(b"typokat-wu0d-diagnostics-v1");
    let mut incompletes = CanonicalBytes::domain(b"typokat-wu0d-incomplete-v1");
    let mut ledger = CanonicalBytes::domain(b"typokat-wu0d-library-ledger-v1");
    let diagnostic_count = records
        .iter()
        .filter(|(_, record)| matches!(record, CheckerRecord::Diagnostic(_)))
        .count();
    diagnostics.usize(diagnostic_count)?;
    incompletes.usize(records.len() - diagnostic_count)?;
    ledger.usize(records.len())?;
    for (key, record) in records {
        let source = sources
            .iter()
            .find(|source| source.file_ordinal == key.file_ordinal)
            .ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "library record has no canonical source".to_owned(),
                )
            })?
            .source;
        let record_bytes = canonical_record_bytes(source, record)?;
        encode_library_key(&mut ledger, *key)?;
        ledger.bytes(&record_bytes)?;
        let component = match record {
            CheckerRecord::Diagnostic(_) => &mut diagnostics,
            CheckerRecord::Incomplete(_) => &mut incompletes,
        };
        encode_library_key(component, *key)?;
        component.bytes(&record_bytes)?;
    }
    Ok(CanonicalLibraryEvidence {
        diagnostics: diagnostics.finish(),
        incompletes: incompletes.finish(),
        ledger: ledger.finish(),
    })
}
