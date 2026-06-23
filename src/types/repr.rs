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

/// A member of an object type. Defined now for M2; not constructed in M0.
#[derive(Clone, Debug)]
#[allow(dead_code)] // TODO(M2): populated by the object/interface checker.
pub struct PropertyType {
    pub name: String,
    pub ty: TypeId,
    pub optional: bool,
}

/// Structural object type (object literal types, interfaces).
/// TODO(M2): interned via `Interner` and compared property-wise by the relation
/// engine (width + depth).
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // TODO(M2)
pub struct ObjectType {
    /// Members in declaration order (display + iteration order, README "Type
    /// display format").
    pub properties: Vec<PropertyType>,
}

/// A single function parameter. Defined now for M3.
#[derive(Clone, Debug)]
#[allow(dead_code)] // TODO(M3)
pub struct ParameterType {
    pub name: String,
    pub ty: TypeId,
    pub optional: bool,
}

/// Structural function type (`(x: number) => string`).
/// TODO(M3): relation engine compares contravariantly on params, covariantly on
/// return (mvp-plan §4.4 / architecture §6.5 — soundness over tsc bivariance).
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // TODO(M3)
pub struct FunctionType {
    pub params: Vec<ParameterType>,
    pub ret: Option<TypeId>,
}
