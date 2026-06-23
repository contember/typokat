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
    /// A union type. `payload` indexes `Store::unions`.
    /// TODO(M4): constructed + canonicalized by `Interner::union`.
    Union,
    /// A function type. `payload` indexes `Store::functions`.
    /// TODO(M3): constructed by the function checker.
    Function,
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

/// A member of an object type.
///
/// M2 properties are all required (`optional` is always `false`); the field is
/// kept so optional members (`a?: T`) slot in later without a struct change.
#[derive(Clone, Debug)]
pub struct PropertyType {
    pub name: String,
    pub ty: TypeId,
    pub optional: bool,
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
