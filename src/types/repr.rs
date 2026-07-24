//! Type representation primitives: `TypeTag`, `TypeFlags`, payload side-table
//! structs, and `LiteralValue`.

use crate::types::store::TypeId;

/// Discriminant for the SoA arena: selects which cold side-table `payload`
/// indexes into. Kept to one byte so the hot `tag` vec is cache-dense.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum TypeTag {
    /// Built-in keyword types (`number`, `string`, …). `payload` is the
    /// `IntrinsicKind` discriminant.
    Intrinsic = 0,
    /// A literal type (`1`, `"x"`, `true`). `payload` indexes `Store::literals`.
    Literal = 1,
    /// An object/interface type. `payload` indexes `Store::objects`.
    /// TODO(M2): constructed by the object-literal / interface checker.
    Object = 2,
    /// A union type. `payload` indexes `Store::unions`. Constructed and
    /// canonicalized by `Interner::union` (M4).
    Union = 3,
    /// An **intersection** type (`A & B` — M31). `payload` indexes
    /// `Store::intersections`. The structural dual of `Union`: canonical member
    /// set, but `never` absorbs and `unknown` is dropped.
    Intersection = 4,
    /// A function type. `payload` indexes `Store::functions`.
    /// TODO(M3): constructed by the function checker.
    Function = 5,
    /// A **type parameter** (`T` in `function f<T>(…)`). `payload` indexes
    /// `Store::type_params`. Constructed by `Interner::intern_type_param` (M9).
    TypeParam = 6,
    /// An **array** type (`T[]` / `Array<T>`). `payload` indexes
    /// `Store::arrays`. Carries a single **element** `TypeId`; interned/hashed by
    /// that element id. Constructed by `Interner::intern_array` (M17).
    Array = 7,
    /// A **tuple** type (`[A, B]`). `payload` indexes `Store::tuples`. Carries an
    /// ordered fixed element list plus an optional rest segment; interned/hashed
    /// by that full shape (order is significant — unlike a union,
    /// `[A, B]` ≠ `[B, A]`).
    /// Constructed by `Interner::intern_tuple` (M18).
    Tuple = 8,
    /// A **readonly** array/tuple wrapper. `payload` stores the wrapped operand
    /// inline; user-written object shapes cannot satisfy it by synthetic property.
    Readonly = 9,
    /// A **conditional** type (`C extends E ? T : F` — M25). Carries component ids,
    /// `infer` binder count, distribution flag, and poison flag; recursive alias
    /// templates are reserved/filled like nominal objects.
    Conditional = 10,
    /// A **lazy alias instantiation** (`Alias<Args>` — M25). Denotes
    /// `substitute(base, args)` computed by the evaluator so self-recursive aliases
    /// do not expand at lowering.
    Instantiation = 11,
    /// An **`infer` binder** (`infer U` inside a conditional's `extends` type — M25).
    /// `payload` is the node-local de Bruijn index. Identity is the index alone, and
    /// substitution never targets it (bound variable; ADR-0002 no-capture rule).
    Infer = 12,
    /// A **mapped type** (`{ [K in S]: V }` — M26). `payload` indexes `Store::mapped`.
    /// Carries the key source, the value template (with `T[K]` represented as the
    /// node-scoped [`TypeTag::MappedValue`] placeholder), and the optional/readonly
    /// modifier arithmetic. A mapped type over a **concrete** key source is *evaluated*
    /// to an object by the type-level evaluator; one over a free declaration type
    /// parameter stays a **deferred** node under the M25 conservative relation rules
    /// (identical-only). Constructed via `Interner::intern_mapped`. See
    /// [`MappedType`].
    Mapped = 13,
    /// The **source value placeholder** (`T[K]`) inside a mapped type's value template.
    /// A node-scoped bound variable: substitution never targets it, and the evaluator
    /// replaces it per key. Identity is the tag alone (payload `0`).
    MappedValue = 14,
    /// A **template literal type** (`` `a${T}` `` — M27). `payload` indexes
    /// `Store::templates`. Carries alternating literal **text** segments and **hole**
    /// `TypeId`s (the interpolated types), the holes being ordinary types folded into the
    /// hash. A template whose holes are all string/number/boolean literals (or unions
    /// thereof) is *constructed* by the type-level evaluator to a string literal or the
    /// cartesian-product union; one with a `string`/`number` intrinsic hole stays a
    /// symbolic **pattern** (anchored segment matching in the relation engine); one with a
    /// free declaration type parameter hole stays a **deferred** node related
    /// conservatively (identical-only, plus deferred → `string`). Constructed via
    /// `Interner::intern_template`. See [`TemplateType`].
    Template = 15,
    /// A **deferred `keyof`** (`keyof T` over a not-yet-computable operand — M28).
    /// `payload` is the **operand** `TypeId` stored inline (like an `Infer` index — no
    /// cold side-table row). Constructed only when the operand is a pending type-level
    /// computation (a free type parameter, a deferred conditional / mapped /
    /// instantiation / template / keyof); a concrete **object** operand keys eagerly at
    /// lowering, and other operand shapes (primitives, unions, arrays, tuples) stay the
    /// M20 out-of-scope error type. Substitution rewrites the operand; the shared
    /// evaluator resolves the node at a value-position demand once the operand is
    /// concrete — through the SAME keyof computation used for eager operands (single
    /// source of truth). An unevaluable node relates conservatively (identical-node
    /// only). Constructed via `Interner::intern_keyof`.
    Keyof = 16,
    /// An immutable application of a class declaration to its effective type
    /// arguments. The class identity and ordered argument list are the complete
    /// structural key; member projection is checker-owned and never alias evaluation.
    ClassInstance = 17,
    /// A deferred indexed access `Object[Index]`. Both ordered operands are part of
    /// identity and are resolved by the class-aware demand boundary, never by the
    /// legacy alias evaluator.
    DeferredIndexedAccess = 18,
    /// An AST-free declaration annotation recipe plus its canonical mapper.
    /// Materialization happens only at the semantic-query demand boundary.
    Declared = 19,
}

impl TypeTag {
    pub const fn discriminant(self) -> u8 {
        match self {
            TypeTag::Intrinsic => 0,
            TypeTag::Literal => 1,
            TypeTag::Object => 2,
            TypeTag::Union => 3,
            TypeTag::Intersection => 4,
            TypeTag::Function => 5,
            TypeTag::TypeParam => 6,
            TypeTag::Array => 7,
            TypeTag::Tuple => 8,
            TypeTag::Readonly => 9,
            TypeTag::Conditional => 10,
            TypeTag::Instantiation => 11,
            TypeTag::Infer => 12,
            TypeTag::Mapped => 13,
            TypeTag::MappedValue => 14,
            TypeTag::Template => 15,
            TypeTag::Keyof => 16,
            TypeTag::ClassInstance => 17,
            TypeTag::DeferredIndexedAccess => 18,
            TypeTag::Declared => 19,
        }
    }
}

/// The fixed intrinsic set. The discriminant is the `TypeTag::Intrinsic`
/// payload, and this order defines the well-known ids.
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
    /// M28 string-manipulation intrinsic markers, used as symbolic instantiation
    /// bases and appended after keyword kinds so existing well-known ids stay stable.
    Uppercase,
    Lowercase,
    Capitalize,
    Uncapitalize,
    /// Backlog 70 contextual-`this` marker. It is only meaningful as the base
    /// of an instantiation inside a contextual intersection.
    ThisType,
    /// Backlog 70 trusted prelude specialization for `OmitThisParameter`.
    OmitThisParameter,
    /// The non-primitive `object` keyword. Kept distinct from the structural
    /// empty object type `{}`, which accepts represented non-nullish primitives.
    Object,
}

impl IntrinsicKind {
    /// The full set in canonical interning order. Adding a kind here is the only
    /// place that defines the well-known id assignment.
    pub const ALL: [IntrinsicKind; 17] = [
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
        IntrinsicKind::Uppercase,
        IntrinsicKind::Lowercase,
        IntrinsicKind::Capitalize,
        IntrinsicKind::Uncapitalize,
        IntrinsicKind::ThisType,
        IntrinsicKind::OmitThisParameter,
        IntrinsicKind::Object,
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
            // M28 string-intrinsic markers: render by their alias name, so a symbolic
            // instantiation displays as `Uppercase<T>`.
            IntrinsicKind::Uppercase => "Uppercase",
            IntrinsicKind::Lowercase => "Lowercase",
            IntrinsicKind::Capitalize => "Capitalize",
            IntrinsicKind::Uncapitalize => "Uncapitalize",
            IntrinsicKind::ThisType => "ThisType",
            IntrinsicKind::OmitThisParameter => "OmitThisParameter",
            IntrinsicKind::Object => "object",
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

/// Stable identity for one class declaration. Stamped onto declared members so
/// private/protected origins make structurally equal classes nominally distinct.
/// Part of a property's structural identity (hashed + compared in the interner).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ClassId(pub u32);

impl ClassId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Class-member access modifier (M13). Together with
/// [`PropertyType::declaring_class`], it is property identity and drives both
/// access-control diagnostics and the nominal relation rule.
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

/// A member of an object type. M13–M15 metadata is identity-bearing so interning
/// preserves nominal origins and write-target behavior; assignability uses
/// visibility/origin but ignores `readonly`/`is_accessor`.
#[derive(Clone, Debug)]
pub struct PropertyType {
    pub name: String,
    /// The type observed by reads and structural assignability.
    pub ty: TypeId,
    /// The type accepted by writes when it differs from [`ty`](Self::ty), as for
    /// paired accessors. Identity-bearing and relation-ignored.
    pub write_ty: Option<TypeId>,
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
    /// Whether the member was declared `readonly` (M14). Identity-bearing, but
    /// relation-ignored; it only gates assignment targets (`TK2540`).
    pub readonly: bool,
    /// Whether this member is a **get/set accessor** rather than a data field (M15).
    /// A **get-only** accessor is modelled as `readonly: true` so member-assignment
    /// reuses the M14 `readonly` machinery — but, unlike a `readonly` *field*, an
    /// accessor is read-only **everywhere, including its declaring class's
    /// constructor** (tsc `TS2540`). This flag distinguishes the two so the
    /// constructor carve-out (`this.prop` assignable in the declaring constructor)
    /// applies to `readonly` fields **only**, never to a get-only accessor.
    ///
    /// Identity-bearing like [`readonly`](PropertyType::readonly), but
    /// relation-ignored. `false` for data fields and object/interface members.
    pub is_accessor: bool,
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
            write_ty: None,
            optional: false,
            visibility: Visibility::Public,
            declaring_class: None,
            readonly: false,
            is_accessor: false,
        }
    }
}

/// Structural object type (object literal types, interfaces). Interned via
/// `Interner::intern_object` (canonicalized: properties sorted by name) and
/// compared property-wise by the relation engine (width + depth).
///
/// M19 index signatures store only the value type for fixed `string`/`number`
/// keys. They coexist with named properties, participate in object identity, and
/// are rewritten by substitution.
///
/// F1/WU2 adds call signatures as interned [`FunctionType`] ids. F1/WU3 mirrors
/// that for construct signatures. Each work unit only lowers a single signature of
/// its kind, but these are `Vec`s so overload work can extend the object shape
/// without another representation split. The signatures coexist with named
/// properties and index signatures and are part of object identity.
#[derive(Clone, Debug, Default)]
pub struct ObjectType {
    /// Members in canonical name order. The interner sorts before hash-consing;
    /// the renderer prints this canonical order.
    pub properties: Vec<PropertyType>,
    /// The **value** type of the string index signature `[k: string]: T` (M19), or
    /// `None` if the object has none. Part of the object's structural identity.
    pub string_index: Option<TypeId>,
    /// The **value** type of the number index signature `[i: number]: T` (M19), or
    /// `None` if the object has none. Part of the object's structural identity.
    pub number_index: Option<TypeId>,
    /// Interned call signatures on this object. WU2 uses length `0` or `1`; a
    /// longer list is reserved for overloads and is not lowered yet. Part of the
    /// object's structural identity.
    pub call_signatures: Vec<TypeId>,
    /// Interned construct signatures on this object. WU3 uses length `0` or `1`;
    /// a longer list is reserved for overloads and is not lowered yet. Part of the
    /// object's structural identity.
    pub construct_signatures: Vec<TypeId>,
}

impl ObjectType {
    /// Look up a property by name, returning its declared type. `O(n)` linear
    /// scan — object types in the subset are small, and a plain `Vec` keeps the
    /// canonical (name-sorted) order the renderer prints.
    pub fn property(&self, name: &str) -> Option<&PropertyType> {
        self.properties.iter().find(|p| p.name == name)
    }
}

/// A single function parameter (M3, signature-shape-expanded in M32/WU2).
///
/// The `name` is retained for the renderer — function types display their
/// parameter names (`(x: number) => string`, README "Type display format").
/// Required-only parameters should be built with [`ParameterType::required`] so
/// the shape flags default consistently.
///
/// FLAG: keeping the name in the interned function type means two function types
/// that differ only in a parameter *name* (`(a: number) => void` vs
/// `(b: number) => void`) are *not* deduplicated, which diverges from strict TS
/// structural identity (parameter names are not part of a function type's
/// identity). This is intentional and reversible: the README mandates that the
/// renderer print the source parameter names, and no fixture relies on
/// name-insensitive function identity. The relation engine ignores names (it
/// matches parameters positionally), so assignability is unaffected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterType {
    pub name: String,
    pub ty: TypeId,
    /// Whether the parameter may be omitted at call sites (`x?: T`). Defaulted
    /// parameters also set this because their call-shape semantics are optional.
    pub optional: bool,
    /// Whether this optional parameter came from a default initializer (`x = v`).
    /// The initializer expression is not a type child, but the shape identity is.
    pub has_default: bool,
    /// Whether this is the signature's rest parameter (`...args: T`).
    pub rest: bool,
}

impl ParameterType {
    /// Build a required fixed parameter `name: ty`.
    pub fn required(name: impl Into<String>, ty: TypeId) -> Self {
        ParameterType {
            name: name.into(),
            ty,
            optional: false,
            has_default: false,
            rest: false,
        }
    }

    /// Build an optional fixed parameter `name?: ty`.
    pub fn optional(name: impl Into<String>, ty: TypeId) -> Self {
        ParameterType {
            name: name.into(),
            ty,
            optional: true,
            has_default: false,
            rest: false,
        }
    }

    /// Build a defaulted fixed parameter `name = ...`.
    pub fn defaulted(name: impl Into<String>, ty: TypeId) -> Self {
        ParameterType {
            name: name.into(),
            ty,
            optional: true,
            has_default: true,
            rest: false,
        }
    }

    /// Build a rest parameter `...name: ty`.
    pub fn rest(name: impl Into<String>, ty: TypeId) -> Self {
        ParameterType {
            name: name.into(),
            ty,
            optional: false,
            has_default: false,
            rest: true,
        }
    }

    /// Whether this fixed parameter contributes to the required call arity.
    pub fn is_required_fixed(&self) -> bool {
        !self.rest && !self.optional
    }

    /// Whether this parameter is a non-rest positional slot.
    pub fn is_fixed(&self) -> bool {
        !self.rest
    }
}

/// A unique identifier for one type-parameter **declaration site** (the `T` in
/// `function f<T>(…)`, `interface Box<T>`, `type Pair<A, B>`). Allocated per
/// declared type parameter by the checker and embedded in the type-parameter
/// type so substitution can target it.
///
/// FLAGGED DEVIATION (architecture §3.1 / ADR-0002):
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

/// A type-parameter type. Identity is [`TypeParamId`] alone; `name` is
/// rendering-only. Instantiation replaces it by substitution.
#[derive(Clone, Debug)]
pub struct TypeParamType {
    /// The unique id of the declaring type parameter — the substitution key.
    pub id: TypeParamId,
    /// The source name (`T`), kept for diagnostics/rendering only; never part of
    /// the type's identity.
    pub name: String,
}

/// One persistent binder owned by a generic function signature.
///
/// The id preserves the named-unique declaration-parameter invariant. Constraint
/// and default types belong here as well: substituting an outer generic parameter
/// must not lose or stale a method binder's call-observable shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericTypeParam {
    pub id: TypeParamId,
    pub constraint: Option<TypeId>,
    pub default: Option<TypeId>,
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
    /// Ordered method/function-local binders. Names remain on their interned
    /// [`TypeParamType`] nodes for display only; ids, constraints, and defaults
    /// participate in the function's structural identity.
    pub type_params: Vec<GenericTypeParam>,
    /// An explicit `this: T` receiver. It is not a positional call argument and
    /// therefore does not contribute to arity or rest-parameter shape.
    pub receiver: Option<TypeId>,
    pub params: Vec<ParameterType>,
    pub ret: TypeId,
}

impl FunctionType {
    /// Required positional parameter count, excluding rest parameters.
    pub fn required_param_count(&self) -> usize {
        self.params
            .iter()
            .filter(|param| param.is_required_fixed())
            .count()
    }

    /// Total number of non-rest positional parameters.
    pub fn total_fixed_param_count(&self) -> usize {
        self.params.iter().filter(|param| param.is_fixed()).count()
    }

    /// Fixed parameters in source order.
    pub fn fixed_params(&self) -> impl Iterator<Item = &ParameterType> {
        self.params.iter().filter(|param| param.is_fixed())
    }

    /// The represented rest parameter, if any.
    pub fn rest_param(&self) -> Option<&ParameterType> {
        self.params.iter().find(|param| param.rest)
    }

    pub fn has_rest_param(&self) -> bool {
        self.rest_param().is_some()
    }
}

/// An array type (`T[]` / `Array<T>`) — M17. Identity is the element id; arrays
/// relate covariantly (matching tsc), and substitution rewrites the element.
#[derive(Copy, Clone, Debug)]
pub struct ArrayType {
    /// The element type — the `T` of `T[]`. The array's entire structural identity.
    pub element: TypeId,
}

/// A tuple rest segment (`...T`) at a source-order position among fixed elements.
/// The type is the represented rest container (`B[]`, `T`, or a tuple type), not
/// an optional tuple element. Optional tuple elements remain out of scope.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TupleRestType {
    /// Insertion position among [`TupleType::elements`]. `0` represents
    /// `[...T, X]`, `elements.len()` represents `[X, ...T]`.
    pub position: usize,
    pub ty: TypeId,
}

impl TupleRestType {
    pub fn new(position: usize, ty: TypeId) -> Self {
        TupleRestType { position, ty }
    }
}

/// A tuple type (`[A, B]`) — M18, extended in M32/WU2 with one represented rest
/// segment. Identity is the ordered fixed element list plus the optional rest
/// position/type; fixed elements are never sorted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TupleType {
    /// Fixed element types in source order, excluding the optional rest segment.
    pub elements: Vec<TypeId>,
    pub rest: Option<TupleRestType>,
}

impl TupleType {
    /// Build a fixed tuple `[A, B]`.
    pub fn fixed(elements: Vec<TypeId>) -> Self {
        TupleType {
            elements,
            rest: None,
        }
    }

    /// Build a tuple with one rest segment.
    pub fn with_rest(elements: Vec<TypeId>, rest: TupleRestType) -> Self {
        TupleType {
            elements,
            rest: Some(rest),
        }
    }

    pub fn fixed_len(&self) -> usize {
        self.elements.len()
    }

    pub fn has_rest(&self) -> bool {
        self.rest.is_some()
    }
}

/// A conditional type `check extends extends_ty ? true_branch : false_branch` (M25).
///
/// The four component ids plus [`infer_count`](ConditionalType::infer_count) and
/// [`distributive`](ConditionalType::distributive) are its whole structural identity.
/// **Field order is meaning** — the interner must not reorder or sort them (unlike a
/// union's members): swapping the branches is a different type. `infer` binders inside
/// the `extends` type are represented as [`TypeTag::Infer`] de Bruijn indices scoped to
/// this node (ADR-0002), and are in scope only in `true_branch` (a reference in
/// `false_branch` is out of scope → `TK2304` at lowering).
#[derive(Copy, Clone, Debug)]
pub struct ConditionalType {
    /// The **check** type (`C` in `C extends E ? T : F`). A conditional is *deferred*
    /// while this contains a free declaration type parameter, and *evaluated* once it is
    /// concrete.
    pub check: TypeId,
    /// The **extends** type (`E`) — the constraint the check is tested against. May
    /// contain [`TypeTag::Infer`] binders.
    pub extends_ty: TypeId,
    /// The **true** branch (`T`), taken when `check <: extends_ty`. May reference this
    /// node's infer binders (substituted with their matched candidates on selection).
    pub true_branch: TypeId,
    /// The **false** branch (`F`), taken otherwise. Infer binders are out of scope here.
    pub false_branch: TypeId,
    /// The number of distinct `infer` binders this node introduces (de Bruijn indices
    /// `0..infer_count`). `0` for a conditional with no `infer`.
    pub infer_count: u32,
    /// Whether the check type was a **naked** declaration type parameter at lowering
    /// (`T extends …`, not `[T] extends …` or `(T | undefined) extends …`). This is what
    /// makes the conditional **distributive**: an instantiation whose argument is a union
    /// (or `never`, or the `boolean` intrinsic) distributes over its members. Recorded at
    /// lowering because substitution erases the naked parameter.
    pub distributive: bool,
    /// Whether this node is **poisoned** by a cross-binder `infer` reference (backlog 26
    /// stopgap): a reference to an OUTER conditional's `infer` binder from inside a
    /// nested node poisons every node from the reference up to and including the
    /// binder-owning one. A poisoned conditional **never evaluates** — it stays a
    /// deferred node under the conservative relation rules (over-report; tsc resolves
    /// these). Declaration-param substitution still applies (only evaluation is off).
    /// Identity-bearing like [`distributive`](ConditionalType::distributive) (folded
    /// into the hash/eq).
    pub poisoned: bool,
}

/// Lazy alias instantiation `substitute(base, args)` (M25). `args` is sorted by
/// [`TypeParamId`] for stable identity; laziness prevents recursive aliases from
/// expanding at lowering.
#[derive(Clone, Debug)]
pub struct InstantiationType {
    /// The conditional template being applied (its own free parameters are the keys of
    /// `args`).
    pub base: TypeId,
    /// The substitution `TypeParamId → TypeId`, **sorted by the id** so two equal
    /// instantiations hash-cons to one node.
    pub args: Vec<(TypeParamId, TypeId)>,
}

/// Immutable class application. Arguments remain in declaration-parameter order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassInstanceType {
    pub class: ClassId,
    pub args: Vec<TypeId>,
}

/// Immutable deferred indexed access. Operand order is semantic.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DeferredIndexedAccessType {
    pub object: TypeId,
    pub index: TypeId,
}

/// Stable identity of an AST-free declaration annotation recipe.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeclaredRecipeId(pub u32);

impl DeclaredRecipeId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The supported declaration-surface syntax after query-free planning.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DeclaredRecipeNode {
    Type(TypeId),
    Array(DeclaredRecipeId),
    Tuple {
        elements: Vec<DeclaredRecipeId>,
        rest: Option<(usize, DeclaredRecipeId)>,
    },
    Readonly(DeclaredRecipeId),
    Application {
        template: TypeId,
        parameters: Vec<TypeParamId>,
        arguments: Vec<DeclaredRecipeId>,
    },
}

/// One hash-consed recipe and its canonical free declaration parameters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeclaredTypeRecipe {
    pub node: DeclaredRecipeNode,
    pub free_params: Vec<TypeParamId>,
}

/// A declaration recipe application. Mapper keys are sorted, unique, and
/// restricted to the recipe's free parameters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeclaredType {
    pub recipe: DeclaredRecipeId,
    pub mapper: Vec<(TypeParamId, TypeId)>,
}

/// Mapped-type modifier arithmetic (`?`/`readonly`) — identity-bearing and folded
/// into mapped-node hash/eq.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum ModifierOp {
    /// No modifier written — keep the source property's flag (homomorphic
    /// preservation); the non-homomorphic default flag is `false`.
    Keep = 0,
    /// `?` / `+?` (optional) or `readonly` / `+readonly` — set the flag.
    Add = 1,
    /// `-?` or `-readonly` — clear the flag.
    Remove = 2,
}

impl ModifierOp {
    /// Apply the modifier to a source flag (M26): `Keep` preserves it, `Add` sets it,
    /// `Remove` clears it.
    pub fn apply(self, source: bool) -> bool {
        match self {
            ModifierOp::Keep => source,
            ModifierOp::Add => true,
            ModifierOp::Remove => false,
        }
    }
}

/// A template literal type `` `a${T}b${U}c` `` (M27).
///
/// Stored as alternating **text** segments and **hole** types: `texts[i]` precedes
/// `holes[i]`, and `texts.last()` is the trailing text, so `texts.len() ==
/// holes.len() + 1` always. `` `a-${T}` `` is `texts = ["a-", ""]`, `holes = [T]`;
/// `` `${A}${B}` `` (adjacent holes, no separator) is `texts = ["", "", ""]`,
/// `holes = [A, B]` — the empty **interior** text is what records the adjacency the
/// M27 poison rule reads. The whole `(texts, holes)` pair is the structural identity
/// (holes are ordinary types, folded into the hash), so two structurally equal
/// templates hash-cons to one id and substitution may make a hole concrete.
#[derive(Clone, Debug, Default)]
pub struct TemplateType {
    /// The literal text segments in order (`texts.len() == holes.len() + 1`). A leading
    /// / trailing empty string means the template starts / ends with a hole; an empty
    /// **interior** string means two adjacent holes with no separator.
    pub texts: Vec<String>,
    /// The interpolated hole types, in order. Each is an ordinary interned type
    /// (`string`/`number` intrinsic, a literal or union thereof, a free type parameter,
    /// or an `infer` binder inside a conditional's extends position).
    pub holes: Vec<TypeId>,
}

/// Render an `f64` like JavaScript `String(n)`: `-0` becomes `"0"` and non-finite
/// values use JavaScript spellings.
pub fn number_to_string(n: f64) -> String {
    if n == 0.0 {
        // Covers both `0.0` and `-0.0` (JS `String(-0)` is `"0"`).
        return "0".to_string();
    }
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }
    let mut buffer = dragonbox_ecma::Buffer::new();
    buffer.format_finite(n).to_string()
}

/// Render a decimal candidate with Rust's historical `Display` behavior.
///
/// Template `${number}` matching deliberately keeps this separate from JavaScript
/// `Number::toString`: TypeScript accepts some decimal spellings such as the fixed
/// representation of `1e21` in the intrinsic pattern.
pub(crate) fn decimal_number_to_string(n: f64) -> String {
    format!("{n}")
}

/// A mapped type `{ [K in S]: V }` (M26).
///
/// The whole struct is its structural identity (all fields folded into the hash/eq).
/// A mapped type is *evaluated* to an object once its [`key_source`](MappedType::key_source)
/// is concrete (no free declaration type parameter); while it contains a free
/// parameter it stays a **deferred** node related conservatively (M25 model — identical
/// node only). The value template's `T[K]` is the node-scoped
/// [`TypeTag::MappedValue`] placeholder, left untouched by declaration-param
/// substitution (no-capture) and resolved per key at evaluation.
#[derive(Copy, Clone, Debug)]
pub struct MappedType {
    /// Whether the `in` clause was `keyof <source>` (**homomorphic**). A homomorphic
    /// map iterates the source object's properties and **preserves** each property's
    /// `?`/`readonly` flags (before the modifier arithmetic); a non-homomorphic map
    /// (a literal-union key source) builds required, non-readonly members by default.
    pub homomorphic: bool,
    /// The key source. When [`homomorphic`](MappedType::homomorphic): the object
    /// **source** operand `<source>` of `keyof <source>` (was `T`, substituted to a
    /// concrete object at instantiation). Otherwise: the constraint type directly (the
    /// literal-union / single-literal key set, e.g. `"x" | "y"`).
    pub key_source: TypeId,
    /// The value type template, with the current key's source property value (`T[K]`)
    /// represented as the [`TypeTag::MappedValue`] placeholder.
    pub value_template: TypeId,
    /// The **modifiers source** of a NON-homomorphic map whose value template indexes an
    /// object by the mapped key (M28 — tsc's `modifiersType`): the `T` of
    /// `{ [P in K]: T[P] }`, captured at lowering when the indexed-access object is a
    /// bare in-scope type parameter. At evaluation each key resolves against this
    /// object — its property's value type replaces the `MappedValue` placeholder and its
    /// `?`/`readonly` flags seed the modifier arithmetic (so `Pick` preserves both).
    /// `None` for a homomorphic map (its `key_source` IS the modifiers source) and for
    /// value templates with no key-indexed access (`Record`'s bare `V`). Identity-bearing
    /// (folded into the hash/eq): two maps differing only here are distinct types.
    pub modifiers_source: Option<TypeId>,
    /// The optionality modifier (`?`/`+?`/`-?`), applied to each property's starting
    /// optional flag.
    pub optional_modifier: ModifierOp,
    /// The readonly modifier (`readonly`/`+readonly`/`-readonly`), applied to each
    /// property's starting readonly flag.
    pub readonly_modifier: ModifierOp,
}

#[cfg(test)]
mod tests {
    use super::{decimal_number_to_string, number_to_string, TypeTag};

    #[test]
    fn type_tag_discriminants_are_source_pinned_and_append_only() {
        let tags = [
            TypeTag::Intrinsic,
            TypeTag::Literal,
            TypeTag::Object,
            TypeTag::Union,
            TypeTag::Intersection,
            TypeTag::Function,
            TypeTag::TypeParam,
            TypeTag::Array,
            TypeTag::Tuple,
            TypeTag::Readonly,
            TypeTag::Conditional,
            TypeTag::Instantiation,
            TypeTag::Infer,
            TypeTag::Mapped,
            TypeTag::MappedValue,
            TypeTag::Template,
            TypeTag::Keyof,
            TypeTag::ClassInstance,
            TypeTag::DeferredIndexedAccess,
        ];
        assert_eq!(
            tags.map(TypeTag::discriminant),
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]
        );
    }

    /// ECMAScript Number::toString uses different fixed/exponential thresholds than
    /// Rust's display formatter.
    #[test]
    fn number_to_string_matches_ecmascript_boundary_matrix() {
        let cases = [
            (1.0, "1"),
            (42.0, "42"),
            (3.5, "3.5"),
            (0.0, "0"),
            (-0.0, "0"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (0.5, "0.5"),
            (1e20, "100000000000000000000"),
            (1e21, "1e+21"),
            (-1e21, "-1e+21"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (-1e-7, "-1e-7"),
            (f64::MAX, "1.7976931348623157e+308"),
            (f64::from_bits(1), "5e-324"),
            (0.1 + 0.2, "0.30000000000000004"),
        ];

        for (number, expected) in cases {
            assert_eq!(number_to_string(number), expected, "{number:?}");
        }
    }

    #[test]
    fn decimal_number_to_string_keeps_pattern_round_trip_behavior() {
        assert_eq!(decimal_number_to_string(1e20), "100000000000000000000");
        assert_eq!(decimal_number_to_string(1e21), "1000000000000000000000");
    }
}
