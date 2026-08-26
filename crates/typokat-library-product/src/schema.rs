use crate::keys::{
    CallableScopeKey, ContainerKey, DeclarationKey, GroupKey, MemberKey, RegionKey, SignatureKey,
    SourceKey, StringKey, TypeParameterKey, TypeRecipeKey,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceClassification {
    Script,
    ExternalModule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSite {
    pub source: SourceKey,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualifiedPathRole {
    Reference,
    Query,
    ImportQualifier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualifiedPathDeferredReason {
    Import,
    Enum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualifiedPathUnresolvedCause {
    MissingRoot,
    TypeOnlyRoot,
    MissingMember,
    TypeOnlyIntermediate,
    ValueOnlyLeaf,
    NamespaceOnlyLeaf,
    Unavailable,
    Deferred(QualifiedPathDeferredReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualifiedPathResolutionOutcome {
    Resolved,
    Unresolved {
        cause: QualifiedPathUnresolvedCause,
        failing_segment_ordinal: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedPathRecipe {
    pub recipe: TypeRecipeKey,
    pub role: QualifiedPathRole,
    pub resolution: QualifiedPathResolutionOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedPathSegmentRecipe {
    pub recipe: TypeRecipeKey,
    pub ordinal: u32,
    pub name: StringKey,
    pub site: SourceSite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionKind {
    Source,
    Namespace,
    GlobalAugmentation,
    AmbientExternalModule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerKind {
    CompilationGlobal,
    ExternalModule,
    Namespace,
    AmbientExternalModule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerRecipe {
    pub parent: Option<ContainerKey>,
    pub kind: ContainerKind,
    pub name: Option<StringKey>,
    pub anchor: Option<SourceSite>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRecipe {
    pub ordinal: u32,
    pub name: StringKey,
    pub sha256: StringKey,
    pub byte_len: u32,
    pub classification: SourceClassification,
    pub root_region: RegionKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionRecipe {
    pub parent: Option<RegionKey>,
    pub container: ContainerKey,
    pub kind: RegionKind,
    pub site: SourceSite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalScopeParent {
    Region(RegionKey),
    Callable(CallableScopeKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallableScopeRecipe {
    pub site: SourceSite,
    pub parent: LexicalScopeParent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclarationLexicalScope {
    None,
    Region(RegionKey),
    Callable(CallableScopeKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclarationKind {
    Variable,
    Function,
    Class,
    Parameter,
    CatchParameter,
    Import,
    TypeAlias,
    Interface,
    Enum,
    Namespace,
    ImportEquals,
    NamespaceExport,
    Global,
    DefaultExportExpression,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeclarationFlags {
    pub ambient: bool,
    pub exported: bool,
    pub default_export: bool,
    pub constant: bool,
    pub readonly: bool,
    pub r#abstract: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclarationPayload {
    Variable {
        annotation: Option<TypeRecipeKey>,
    },
    Function {
        signature: SignatureKey,
    },
    Class {
        type_parameters: Box<[TypeParameterKey]>,
        members: Box<[MemberKey]>,
        heritage: Box<[TypeRecipeKey]>,
    },
    Interface {
        type_parameters: Box<[TypeParameterKey]>,
        members: Box<[MemberKey]>,
        heritage: Box<[TypeRecipeKey]>,
    },
    TypeAlias {
        type_parameters: Box<[TypeParameterKey]>,
        value: TypeRecipeKey,
    },
    Enum {
        members: Box<[MemberKey]>,
    },
    Namespace {
        child_region: RegionKey,
    },
    Import,
    ImportEquals,
    NamespaceExport,
    Global {
        child_region: RegionKey,
    },
    Parameter {
        annotation: Option<TypeRecipeKey>,
    },
    CatchParameter {
        annotation: Option<TypeRecipeKey>,
    },
    DefaultExportExpression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationRecipe {
    pub site: SourceSite,
    pub binding_site: SourceSite,
    pub region: RegionKey,
    pub lexical_scope: DeclarationLexicalScope,
    pub name: StringKey,
    pub spaces: DeclarationSlots,
    pub flags: DeclarationFlags,
    pub payload: DeclarationPayload,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeclarationSlots {
    pub value: bool,
    pub r#type: bool,
    pub namespace: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationGroupRecipe {
    pub container: ContainerKey,
    pub name: StringKey,
    pub slots: DeclarationSlots,
    pub fragments: Box<[DeclarationKey]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamespaceAttachmentRecipe {
    pub group: GroupKey,
    pub declaration: DeclarationKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueAttachmentRecipe {
    pub group: GroupKey,
    pub declaration: DeclarationKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntrinsicMarker {
    Uppercase,
    Lowercase,
    Capitalize,
    Uncapitalize,
    NoInfer,
    BuiltinIteratorReturn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntrinsicMarkerRecipe {
    pub declaration: DeclarationKey,
    pub marker: IntrinsicMarker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variance {
    Invariant,
    In,
    Out,
    InOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeParameterRecipe {
    pub site: SourceSite,
    pub owner: SourceSite,
    pub visibility: Box<[SourceSite]>,
    pub name: StringKey,
    pub variance: Variance,
    pub constant: bool,
    pub constraint: Option<TypeRecipeKey>,
    pub default: Option<TypeRecipeKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeywordKind {
    Any,
    Unknown,
    Never,
    Void,
    Null,
    Undefined,
    Boolean,
    Number,
    BigInt,
    String,
    Symbol,
    Object,
    Intrinsic,
    This,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralRecipe {
    String(StringKey),
    NumberBits(u64),
    BigInt(StringKey),
    Boolean(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeOperator {
    Keyof,
    Readonly,
    Unique,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappedModifier {
    Preserve,
    Add,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRecipeNode {
    Keyword(KeywordKind),
    Literal(LiteralRecipe),
    Reference {
        path: Box<[StringKey]>,
        resolution: TypeReferenceResolution,
        arguments: Box<[TypeRecipeKey]>,
        dependency: DependencyLinkRecipe,
    },
    Query {
        path: Box<[StringKey]>,
        resolution: ValueReferenceResolution,
        dependency: DependencyLinkRecipe,
    },
    Import {
        argument: StringKey,
        qualifier: Box<[StringKey]>,
        arguments: Box<[TypeRecipeKey]>,
    },
    Array(TypeRecipeKey),
    Tuple {
        elements: Box<[TupleElementRecipe]>,
    },
    Union(Box<[TypeRecipeKey]>),
    Intersection(Box<[TypeRecipeKey]>),
    Function(SignatureKey),
    Constructor {
        signature: SignatureKey,
        r#abstract: bool,
    },
    Object(Box<[MemberKey]>),
    Operator {
        operator: TypeOperator,
        operand: TypeRecipeKey,
    },
    IndexedAccess {
        object: TypeRecipeKey,
        index: TypeRecipeKey,
    },
    Mapped {
        parameter: TypeParameterKey,
        name: Option<TypeRecipeKey>,
        value: Option<TypeRecipeKey>,
        optional: MappedModifier,
        readonly: MappedModifier,
    },
    Conditional {
        check: TypeRecipeKey,
        extends: TypeRecipeKey,
        when_true: TypeRecipeKey,
        when_false: TypeRecipeKey,
    },
    Infer(TypeParameterKey),
    Template {
        texts: Box<[StringKey]>,
        holes: Box<[TypeRecipeKey]>,
    },
    Predicate {
        parameter: StringKey,
        asserts: bool,
        value: Option<TypeRecipeKey>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeReferenceResolution {
    Group(GroupKey),
    TypeParameter(TypeParameterKey),
    Unresolved { region: RegionKey },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueReferenceResolution {
    Group(GroupKey),
    IntrinsicGlobalValue,
    Unresolved { region: RegionKey },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeRecipe {
    pub site: SourceSite,
    pub node: TypeRecipeNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TupleElementRecipe {
    pub name: Option<StringKey>,
    pub value: TypeRecipeKey,
    pub optional: bool,
    pub rest: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterRecipe {
    pub pattern: ParameterPattern,
    pub value: Option<TypeRecipeKey>,
    pub optional: bool,
    pub rest: bool,
    pub this_parameter: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParameterPattern {
    Identifier(StringKey),
    Array(Box<[StringKey]>),
    Object(Box<[StringKey]>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureRecipe {
    pub type_parameters: Box<[TypeParameterKey]>,
    pub parameters: Box<[ParameterRecipe]>,
    pub result: Option<TypeRecipeKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberKind {
    Property,
    Method,
    Getter,
    Setter,
    Call,
    Construct,
    Index,
    Constructor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accessibility {
    Public,
    Protected,
    Private,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolicValueNameRecipe {
    Path {
        path: Box<[StringKey]>,
        resolution: ValueReferenceResolution,
        dependency: DependencyLinkRecipe,
    },
    String(StringKey),
    NumberBits(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyNameRecipe {
    Identifier(StringKey),
    String(StringKey),
    NumberBits(u64),
    Private(StringKey),
    Computed {
        site: SourceSite,
        value: SymbolicValueNameRecipe,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberRecipe {
    pub site: SourceSite,
    pub kind: MemberKind,
    pub name: Option<PropertyNameRecipe>,
    pub optional: bool,
    pub readonly: bool,
    pub static_member: bool,
    pub accessibility: Option<Accessibility>,
    pub r#abstract: bool,
    pub value: Option<TypeRecipeKey>,
    pub signature: Option<SignatureKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyKind {
    Reference,
    Heritage,
    Query,
    Import,
    IndexedAccess,
    Constraint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DependencyLinkRecipe {
    pub owner: GroupKey,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyTarget {
    Group(GroupKey),
    TypeParameter(TypeParameterKey),
    IntrinsicGlobalValue,
    Unresolved {
        region: RegionKey,
        path: Box<[StringKey]>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyRecipe {
    pub owner: GroupKey,
    pub target: DependencyTarget,
    pub kind: DependencyKind,
    pub site: SourceSite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalStepKind {
    Declaration,
    Heritage,
    Member,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexicalStepRecipe {
    pub owner: DeclarationKey,
    pub kind: LexicalStepKind,
    pub site: SourceSite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryDeclarationProduct {
    pub(crate) profile: StringKey,
    pub(crate) strings: Box<[String]>,
    pub(crate) sources: Box<[SourceRecipe]>,
    pub(crate) containers: Box<[ContainerRecipe]>,
    pub(crate) regions: Box<[RegionRecipe]>,
    pub(crate) callable_scopes: Box<[CallableScopeRecipe]>,
    pub(crate) declarations: Box<[DeclarationRecipe]>,
    pub(crate) groups: Box<[DeclarationGroupRecipe]>,
    pub(crate) namespaces: Box<[NamespaceAttachmentRecipe]>,
    pub(crate) values: Box<[ValueAttachmentRecipe]>,
    pub(crate) intrinsic_markers: Box<[IntrinsicMarkerRecipe]>,
    pub(crate) qualified_paths: Box<[QualifiedPathRecipe]>,
    pub(crate) qualified_path_segments: Box<[QualifiedPathSegmentRecipe]>,
    pub(crate) type_parameters: Box<[TypeParameterRecipe]>,
    pub(crate) types: Box<[TypeRecipe]>,
    pub(crate) signatures: Box<[SignatureRecipe]>,
    pub(crate) members: Box<[MemberRecipe]>,
    pub(crate) dependencies: Box<[DependencyRecipe]>,
    pub(crate) lexical_steps: Box<[LexicalStepRecipe]>,
}

impl LibraryDeclarationProduct {
    pub fn source_key_at(&self, index: usize) -> Option<SourceKey> {
        (index < self.sources.len())
            .then(|| SourceKey::new(index))
            .flatten()
    }
    pub fn container_key_at(&self, index: usize) -> Option<ContainerKey> {
        (index < self.containers.len())
            .then(|| ContainerKey::new(index))
            .flatten()
    }
    pub fn region_key_at(&self, index: usize) -> Option<RegionKey> {
        (index < self.regions.len())
            .then(|| RegionKey::new(index))
            .flatten()
    }
    pub fn callable_scope_key_at(&self, index: usize) -> Option<CallableScopeKey> {
        (index < self.callable_scopes.len())
            .then(|| CallableScopeKey::new(index))
            .flatten()
    }
    pub fn declaration_key_at(&self, index: usize) -> Option<DeclarationKey> {
        (index < self.declarations.len())
            .then(|| DeclarationKey::new(index))
            .flatten()
    }
    pub fn group_key_at(&self, index: usize) -> Option<GroupKey> {
        (index < self.groups.len())
            .then(|| GroupKey::new(index))
            .flatten()
    }
    pub fn type_recipe_key_at(&self, index: usize) -> Option<TypeRecipeKey> {
        (index < self.types.len())
            .then(|| TypeRecipeKey::new(index))
            .flatten()
    }
    pub fn profile_identity(&self) -> &str {
        &self.strings[self.profile.index()]
    }
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
    pub fn declaration_count(&self) -> usize {
        self.declarations.len()
    }
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }
    pub fn type_recipe_count(&self) -> usize {
        self.types.len()
    }
    pub fn dependency_count(&self) -> usize {
        self.dependencies.len()
    }
    pub fn strings(&self) -> &[String] {
        &self.strings
    }
    pub fn sources(&self) -> &[SourceRecipe] {
        &self.sources
    }
    pub fn containers(&self) -> &[ContainerRecipe] {
        &self.containers
    }
    pub fn regions(&self) -> &[RegionRecipe] {
        &self.regions
    }
    pub fn callable_scopes(&self) -> &[CallableScopeRecipe] {
        &self.callable_scopes
    }
    pub fn declarations(&self) -> &[DeclarationRecipe] {
        &self.declarations
    }
    pub fn groups(&self) -> &[DeclarationGroupRecipe] {
        &self.groups
    }
    pub fn type_parameters(&self) -> &[TypeParameterRecipe] {
        &self.type_parameters
    }
    pub fn types(&self) -> &[TypeRecipe] {
        &self.types
    }
    pub fn members(&self) -> &[MemberRecipe] {
        &self.members
    }
    pub fn signatures(&self) -> &[SignatureRecipe] {
        &self.signatures
    }
    pub fn lexical_steps(&self) -> &[LexicalStepRecipe] {
        &self.lexical_steps
    }
    pub fn dependency_rows(&self) -> &[DependencyRecipe] {
        &self.dependencies
    }
    pub fn namespace_attachment_rows(&self) -> &[NamespaceAttachmentRecipe] {
        &self.namespaces
    }
    pub fn value_attachment_rows(&self) -> &[ValueAttachmentRecipe] {
        &self.values
    }
    pub fn intrinsic_marker_rows(&self) -> &[IntrinsicMarkerRecipe] {
        &self.intrinsic_markers
    }
    pub fn qualified_path_rows(&self) -> &[QualifiedPathRecipe] {
        &self.qualified_paths
    }
    pub fn qualified_path_segment_rows(&self) -> &[QualifiedPathSegmentRecipe] {
        &self.qualified_path_segments
    }
}
