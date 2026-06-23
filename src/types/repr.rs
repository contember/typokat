//! Type representation primitives: `TypeTag`, `TypeFlags`, payload side-table
//! structs, and `LiteralValue`.
//!
//! These are the "shapes" the architecture (§3.1) and mvp-plan (§4.1) ask for.
//! M0 only ever constructs `Intrinsic` and `Literal` types, but the cold
//! side-table structs (`ObjectType`, `FunctionType`) are defined now so later
//! milestones (M2/M3) slot in without touching the store layout.

use crate::types::store::TypeId;

/// Discriminant for the SoA arena: selects which cold side-table `payload`
/// indexes into. Kept to one byte so the hot `tag` vec is cache-dense.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum TypeTag {
    /// Built-in keyword types (`number`, `string`, …). `payload` is the
    /// `IntrinsicKind` discriminant.
    Intrinsic,
    /// A literal type (`1`, `"x"`, `true`). `payload` indexes `Store::literals`.
    Literal,
    /// An object/interface type. `payload` indexes `Store::objects`.
    /// TODO(M2): constructed by the object-literal / interface checker.
    Object,
    /// A union type. `payload` indexes `Store::unions`. Constructed and
    /// canonicalized by `Interner::union` (M4).
    Union,
    /// A function type. `payload` indexes `Store::functions`.
    /// TODO(M3): constructed by the function checker.
    Function,
    /// A **type parameter** (`T` in `function f<T>(…)`). `payload` indexes
    /// `Store::type_params`. Constructed by `Interner::intern_type_param` (M9).
    TypeParam,
}

/// The fixed set of intrinsic (keyword) types. The discriminant doubles as the
/// `payload` value for `TypeTag::Intrinsic`, so an intrinsic type is fully
/// described by `(tag = Intrinsic, payload = IntrinsicKind as u32)`.
///
/// Order here is the canonical well-known order; `Interner::with_intrinsics`
/// interns them in this order so each gets a small fixed `TypeId` (§4.1).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum IntrinsicKind {
    /// The error/recovery type. Behaves like `any` for relation purposes and
    /// suppresses cascade diagnostics. M0 never produces it, but it is reserved
    /// as a well-known id so M1's unresolved-name handling (`TK2304`) can use it
    /// without renumbering the intrinsics.
    Error,
    Any,
    Unknown,
    Never,
    Void,
    Null,
    Undefined,
    Boolean,
    Number,
    String,
}

impl IntrinsicKind {
    /// The full set in canonical interning order. Adding a kind here is the only
    /// place that defines the well-known id assignment.
    pub const ALL: [IntrinsicKind; 10] = [
        IntrinsicKind::Error,
        IntrinsicKind::Any,
        IntrinsicKind::Unknown,
        IntrinsicKind::Never,
        IntrinsicKind::Void,
        IntrinsicKind::Null,
        IntrinsicKind::Undefined,
        IntrinsicKind::Boolean,
        IntrinsicKind::Number,
        IntrinsicKind::String,
    ];

    /// Display name used by the type renderer (`tests/cases/README.md` →
    /// "Type display format"). The error type renders as `any` for parity with
    /// how tsc surfaces error types in messages.
    pub fn display_name(self) -> &'static str {
        match self {
            IntrinsicKind::Error => "any",
            IntrinsicKind::Any => "any",
            IntrinsicKind::Unknown => "unknown",
            IntrinsicKind::Never => "never",
            IntrinsicKind::Void => "void",
            IntrinsicKind::Null => "null",
            IntrinsicKind::Undefined => "undefined",
            IntrinsicKind::Boolean => "boolean",
            IntrinsicKind::Number => "number",
            IntrinsicKind::String => "string",
        }
    }
}

/// Packed per-type bit flags (architecture §3.1: SoA hot path). Only the bits
/// M0 needs are defined; the type is a transparent `u32` so adding flags later
/// (e.g. `ContainsError`, `Widened`) is a constant, not a layout change.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct TypeFlags(pub u32);

impl TypeFlags {
    pub const EMPTY: TypeFlags = TypeFlags(0);

    /// This type is or contains the error type; relation checks short-circuit to
    /// success and cascade diagnostics are suppressed.
    /// TODO(M1): set when constructing the error type for unresolved names.
    pub const CONTAINS_ERROR: TypeFlags = TypeFlags(1 << 0);

    #[inline]
    pub fn contains(self, other: TypeFlags) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// A literal type's value. Carries a precomputed 64-bit hash slot in spirit
/// (architecture §3.3) — for M0 the value itself is small and hashed directly in
/// `hash.rs`, so no separate cached hash field is needed yet.
#[derive(Clone, PartialEq, Debug)]
pub enum LiteralValue {
    /// `f64` bit pattern is used for equality/hashing so that `NaN`/`-0.0` are
    /// handled deterministically by the interner (see `hash.rs`).
    Number(f64),
    String(String),
    Boolean(bool),
}

impl LiteralValue {
    /// The intrinsic base type a literal widens to (architecture §6.5 / mvp-plan
    /// §4.4: "literal → base widening"). Used both for the widening relation rule
    /// and for rendering the *widened* form in diagnostics.
    pub fn base_kind(&self) -> IntrinsicKind {
        match self {
            LiteralValue::Number(_) => IntrinsicKind::Number,
            LiteralValue::String(_) => IntrinsicKind::String,
            LiteralValue::Boolean(_) => IntrinsicKind::Boolean,
        }
    }
}

/// A stable identity for one **class declaration** (M13). Allocated per declared
/// `class` by the checker and stamped onto every member that class declares (its
/// [`PropertyType::declaring_class`]), so a `private`/`protected` member can be
/// matched to its *origin* — the declaration it came from. Two structurally
/// identical classes get distinct ids, which is what makes a class with a
/// non-public member **nominal**: `class Secret { private x }` and a structurally
/// equal `class Other { private x }` carry different `ClassId`s on their `x`, so
/// the relation engine (and hash-consing) keep them distinct types.
///
/// A `u32` arena index, mirroring [`TypeParamId`] — cheap to copy/compare and part
/// of a property's structural identity (hashed + compared in the interner).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ClassId(pub u32);

impl ClassId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The access modifier of a class member (M13): `public` (the default),
/// `private`, or `protected`. Read from the member's AST `accessibility`
/// (`PropertyDefinition`/`MethodDefinition`). It is part of a property's
/// **structural identity** (hashed + compared in the interner) together with the
/// [`PropertyType::declaring_class`], so a `private x: number` and a public
/// `x: number` of the same name/type are *different* members — the basis for the
/// access-control checks (`TK2341`/`TK2445`) and the nominal relation rule.
///
/// `Public` is the default and produces no diagnostics; only `Private` and
/// `Protected` drive access control and nominal typing.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Visibility {
    /// `public` (or unannotated) — the structural default, no access control.
    #[default]
    Public,
    /// `private` — accessible only within the declaring class's body (`TK2341`).
    Private,
    /// `protected` — accessible within the declaring class and its subclasses
    /// (`TK2445`).
    Protected,
}

/// A member of an object type.
///
/// M2 properties are all required (`optional` is always `false`); the field is
/// kept so optional members (`a?: T`) slot in later without a struct change.
///
/// M13 adds the member's **visibility** (`public`/`private`/`protected`) and the
/// **declaring class** it originated from. Both are part of the property's
/// structural identity (hashed + compared by the interner): a `private x` and a
/// public `x` of the same name/type are distinct members, and a `private`/
/// `protected` member from one class differs from a same-named one from another
/// (distinct [`ClassId`]). An ordinary structural object literal / interface
/// member is `Public` with no declaring class (`None`), so M0–M12 behaviour is
/// unchanged — those members hash and compare exactly as before.
///
/// M14 adds the [`readonly`](PropertyType::readonly) flag. Like visibility/origin
/// it is part of the property's **structural identity** (hashed + compared by the
/// interner), so it is preserved through interning rather than silently dropped —
/// but, unlike visibility, the **relation engine ignores it** for assignability (a
/// `readonly x` and a mutable `x` relate freely, both directions). It only gates
/// assignment *targets*: assigning to a `readonly` member is `TK2540` (except in
/// the declaring class's constructor). An object-literal / interface member is
/// non-readonly.
#[derive(Clone, Debug)]
pub struct PropertyType {
    pub name: String,
    pub ty: TypeId,
    pub optional: bool,
    /// The member's access modifier (M13). `Public` for object-literal /
    /// interface members and unannotated class members.
    pub visibility: Visibility,
    /// The class this member was declared in (M13), or `None` for a member that
    /// did not come from a class declaration (object literals, interfaces, type
    /// aliases). Together with [`visibility`](PropertyType::visibility) this is the
    /// *origin* the nominal relation rule keys on for `private`/`protected`
    /// members.
    pub declaring_class: Option<ClassId>,
    /// Whether the member was declared `readonly` (M14), read from the AST
    /// modifier (`readonly x: number`). Part of the property's structural identity
    /// (hashed + compared by the interner, like [`visibility`](PropertyType::visibility)),
    /// so it survives interning — but the **relation engine ignores it** for
    /// assignability. It gates only assignment targets: a `readonly` target is
    /// `TK2540` unless it is `this.prop` inside the declaring class's constructor.
    /// `false` for object-literal / interface / unannotated members and methods.
    pub readonly: bool,
}

impl PropertyType {
    /// Build a plain **public**, **mutable** structural property `name: ty` with no
    /// declaring class — the M0–M12 shape. Object-literal, interface, and
    /// substitution code construct members through here so the M13/M14 fields
    /// default consistently.
    pub fn public(name: impl Into<String>, ty: TypeId) -> Self {
        PropertyType {
            name: name.into(),
            ty,
            optional: false,
            visibility: Visibility::Public,
            declaring_class: None,
            readonly: false,
        }
    }
}

/// Structural object type (object literal types, interfaces). Interned via
/// `Interner::intern_object` (canonicalized: properties sorted by name) and
/// compared property-wise by the relation engine (width + depth).
#[derive(Clone, Debug, Default)]
pub struct ObjectType {
    /// Members in **canonical order** (sorted by name). The interner sorts before
    /// hash-consing (mvp-plan §3.3) so `{ a; b }` and `{ b; a }` collapse to one
    /// `TypeId`; the stored order is therefore the canonical (name-sorted) one,
    /// not source order. The renderer prints this canonical order — `; `-separated
    /// (README "Type display format"); object-target messages in the corpus are
    /// asserted code-only, so the exact ordering is never matched against a fixed
    /// layout.
    pub properties: Vec<PropertyType>,
}

impl ObjectType {
    /// Look up a property by name, returning its declared type. `O(n)` linear
    /// scan — object types in the subset are small, and a plain `Vec` keeps the
    /// canonical (name-sorted) order the renderer prints.
    pub fn property(&self, name: &str) -> Option<&PropertyType> {
        self.properties.iter().find(|p| p.name == name)
    }
}

/// A single function parameter (M3).
///
/// `optional` is always `false` in the M3 subset (optional/rest params are
/// deferred); the field is kept so they slot in later without a struct change.
/// The `name` is retained for the renderer — function types display their
/// parameter names (`(x: number) => string`, README "Type display format").
///
/// FLAG: keeping the name in the interned function type means two function types
/// that differ only in a parameter *name* (`(a: number) => void` vs
/// `(b: number) => void`) are *not* deduplicated, which diverges from strict TS
/// structural identity (parameter names are not part of a function type's
/// identity). This is intentional and reversible: the README mandates that the
/// renderer print the source parameter names, and no fixture relies on
/// name-insensitive function identity. The relation engine ignores names (it
/// matches parameters positionally), so assignability is unaffected.
#[derive(Clone, Debug)]
pub struct ParameterType {
    pub name: String,
    pub ty: TypeId,
    pub optional: bool,
}

/// A unique identifier for one type-parameter **declaration site** (the `T` in
/// `function f<T>(…)`, `interface Box<T>`, `type Pair<A, B>`). Allocated per
/// declared type parameter by the checker and embedded in the type-parameter
/// type so substitution can target it.
///
/// FLAGGED DEVIATION (architecture §3.1 / `tests/cases/README.md` "generics"):
/// type parameters use a **named, unique-id** representation, **not de Bruijn
/// indices**. §3.1 calls for de Bruijn so that alpha-equivalent generics
/// (`<T>(x: T) => T` vs `<U>(x: U) => U`) hash-cons to the same node, and for the
/// eventual type-level VM's `infer`. That is a Phase-3 (pre-VM) concern; for M9
/// (explicit type arguments + instantiation by substitution) named unique ids are
/// simpler and *sound*: two distinct generic declarations get distinct ids, so a
/// `TypeParam` never accidentally aliases another declaration's parameter, and
/// substitution is a straightforward `TypeParamId → TypeId` map. The cost is that
/// alpha-equivalent generics do **not** share a node — acceptable now (no fixture
/// relies on it). The migration to de Bruijn, when the VM lands, is localized to
/// this representation plus the substitution routine.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct TypeParamId(pub u32);

impl TypeParamId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A type-parameter type (`T`). Identity is its [`TypeParamId`] alone (the `name`
/// is carried only for rendering — two parameters with the same name but distinct
/// declaration sites have distinct ids and are distinct types). Interned via
/// `Interner::intern_type_param`; instantiation replaces it by substitution
/// (`Interner`/`substitute`, M9).
#[derive(Clone, Debug)]
pub struct TypeParamType {
    /// The unique id of the declaring type parameter — the substitution key.
    pub id: TypeParamId,
    /// The source name (`T`), kept for diagnostics/rendering only; never part of
    /// the type's identity.
    pub name: String,
}

/// Structural function type (`(x: number) => string`).
///
/// Parameters are stored **positionally** (source order, never sorted — only
/// object properties are canonicalized by name). The relation engine compares
/// them contravariantly and the return type covariantly with matching arity
/// (mvp-plan §6.5 / architecture §6.5 — soundness over tsc bivariance for
/// function-typed values). Interned via `Interner::intern_function`.
///
/// No `Default`: `ret` is a real `TypeId` (a function always has a return type —
/// `void` when none is written), and `TypeId` has no meaningful default.
#[derive(Clone, Debug)]
pub struct FunctionType {
    pub params: Vec<ParameterType>,
    pub ret: TypeId,
}
