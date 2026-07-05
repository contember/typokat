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
    /// An **array** type (`T[]` / `Array<T>`). `payload` indexes
    /// `Store::arrays`. Carries a single **element** `TypeId`; interned/hashed by
    /// that element id. Constructed by `Interner::intern_array` (M17).
    Array,
    /// A **tuple** type (`[A, B]`). `payload` indexes `Store::tuples`. Carries an
    /// **ordered** `Vec<TypeId>` of element types; interned/hashed by that ordered
    /// list (order is significant — unlike a union, `[A, B]` ≠ `[B, A]`).
    /// Constructed by `Interner::intern_tuple` (M18).
    Tuple,
    /// A **conditional** type (`C extends E ? T : F` — M25). `payload` indexes
    /// `Store::conditionals`. Carries the four component ids (check, extends, true,
    /// false), the count of `infer` binders it introduces, and whether its check was a
    /// *naked* declaration type parameter (drives distribution). Constructed by
    /// `Interner::intern_conditional`; a recursive conditional alias's template id is
    /// reserved/filled like a nominal object (`reserve_conditional`/`fill_conditional`).
    Conditional,
    /// A **lazy alias instantiation** (`Alias<Args>` where `Alias`'s body is a
    /// conditional — M25). `payload` indexes `Store::instantiations`. Denotes
    /// `substitute(base, args)` computed *lazily* by the evaluator, so a self-recursive
    /// conditional alias (`type Unwrap<T> = … Unwrap<U> …`) does not expand at lowering
    /// (which would loop) but at demand. Constructed by `Interner::intern_instantiation`.
    Instantiation,
    /// An **`infer` binder** (`infer U` inside a conditional's `extends` type — M25).
    /// `payload` is the **de Bruijn index** within the enclosing conditional node
    /// (ADR-0002: de Bruijn scoped to infer binders only). The same infer name in
    /// several positions of one node shares an index. Constructed by
    /// `Interner::intern_infer`; identity is the index alone. `substitute` never targets
    /// it (it is a *bound* variable, not a free declaration parameter — the no-capture
    /// rule).
    Infer,
    /// A **mapped type** (`{ [K in S]: V }` — M26). `payload` indexes `Store::mapped`.
    /// Carries the key source, the value template (with `T[K]` represented as the
    /// node-scoped [`TypeTag::MappedValue`] placeholder), and the optional/readonly
    /// modifier arithmetic. A mapped type over a **concrete** key source is *evaluated*
    /// to an object by the type-level evaluator; one over a free declaration type
    /// parameter stays a **deferred** node under the M25 conservative relation rules
    /// (identical-only). Constructed via `Interner::intern_mapped`. See
    /// [`MappedType`].
    Mapped,
    /// The **source value placeholder** (`T[K]`) inside a mapped type's value template
    /// (M26) — a node-scoped bound variable standing for the current key's source
    /// property value. `substitute` never targets it (a bound variable, not a free
    /// declaration parameter — the no-capture rule, ADR-0002 analog); the evaluator
    /// replaces it per key with the source property's type. Identity is the tag alone
    /// (payload `0`). Constructed via `Interner::intern_mapped_value`.
    MappedValue,
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
    Template,
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
///
/// M15 adds the [`is_accessor`](PropertyType::is_accessor) flag, treated identically
/// at the type-store level (folded into the identity, ignored by the relation), to
/// distinguish a get-only accessor (read-only **everywhere**) from a `readonly` data
/// field (read-only except in its declaring constructor).
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
    /// Whether this member is a **get/set accessor** rather than a data field (M15).
    /// A **get-only** accessor is modelled as `readonly: true` so member-assignment
    /// reuses the M14 `readonly` machinery — but, unlike a `readonly` *field*, an
    /// accessor is read-only **everywhere, including its declaring class's
    /// constructor** (tsc `TS2540`). This flag distinguishes the two so the
    /// constructor carve-out (`this.prop` assignable in the declaring constructor)
    /// applies to `readonly` fields **only**, never to a get-only accessor.
    ///
    /// Like [`readonly`](PropertyType::readonly) it is part of the property's
    /// structural identity (hashed + compared by the interner) so it survives
    /// interning, and the **relation engine ignores it** for assignability (an
    /// accessor property and a same-typed data field relate freely, both ways).
    /// `false` for every data field and for object-literal / interface members.
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
/// M19 adds the two **index signatures** (`{ [k: string]: T }`,
/// `{ [i: number]: T }`). Each is the **value** type of the corresponding index
/// kind (the key type itself is fixed — `string` or `number` — and is not stored).
/// They coexist with named properties and are part of the object's structural
/// identity (hashed + compared by the interner), so `{ [k: string]: number }`
/// interns distinctly from `{}` and from `{ [k: string]: string }`. The relation
/// engine reads them for index-signature assignability; substitution rewrites the
/// value types so a (future) generic `{ [k: string]: T }` instantiates correctly.
///
/// F1/WU2 adds call signatures as interned [`FunctionType`] ids. F1/WU3 mirrors
/// that for construct signatures. Each work unit only lowers a single signature of
/// its kind, but these are `Vec`s so overload work can extend the object shape
/// without another representation split. The signatures coexist with named
/// properties and index signatures and are part of object identity.
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

/// An array type (`T[]` / `Array<T>`) — M17. Carries a single **element**
/// `TypeId`; its whole identity is that element id, so `number[]` is consistent and
/// `number[]` ≠ `string[]`. Interned via `Interner::intern_array`; the relation
/// engine relates two arrays **covariantly** (`S[]` <: `T[]` iff `S` <: `T`, matching
/// tsc's deliberate array covariance), and substitution rewrites the element so a
/// generic `T[]` instantiates correctly.
///
/// A `Copy` newtype rather than a bare `TypeId` so the cold side-table is a distinct
/// type (parallel to `ObjectType`/`FunctionType`) and an array row is never confused
/// with a raw id elsewhere.
#[derive(Copy, Clone, Debug)]
pub struct ArrayType {
    /// The element type — the `T` of `T[]`. The array's entire structural identity.
    pub element: TypeId,
}

/// A tuple type (`[A, B]`) — M18. Carries an **ordered** list of element
/// `TypeId`s; its whole identity is that ordered list, so `[number, string]` is
/// consistent and `[number, string]` ≠ `[string, number]` ≠ `[number]`. Order is
/// significant — the elements are **never sorted** (unlike a union's canonical
/// member set; like a function's positional parameters). Interned via
/// `Interner::intern_tuple`; the relation engine relates two tuples **positionally**
/// (same length AND each element pairwise, `S[i]` <: `T[i]`) and a tuple to an
/// array when every element is assignable to the array element; substitution
/// rewrites each element so a (future) generic tuple instantiates correctly.
///
/// A distinct cold side-table struct (parallel to `ObjectType`/`FunctionType`) so a
/// tuple row is never confused with a union's member slice elsewhere — they hold the
/// same shape (`Vec<TypeId>`) but mean different things (ordered vs canonical set).
#[derive(Clone, Debug, Default)]
pub struct TupleType {
    /// The element types in **source order** (`[A, B]` → `[A, B]`). The tuple's
    /// entire structural identity; never sorted.
    pub elements: Vec<TypeId>,
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

/// A lazy alias instantiation `substitute(base, args)` (M25) — see
/// [`TypeTag::Instantiation`]. `base` is a (reserved-or-resolved) conditional template
/// id; `args` is the substitution to apply, sorted by [`TypeParamId`] for a stable
/// structural identity. Kept lazy so a self-recursive conditional alias does not expand
/// at lowering; the evaluator applies `args` to `base` on demand (and distributes when
/// `base` is distributive and the check argument is a union).
#[derive(Clone, Debug)]
pub struct InstantiationType {
    /// The conditional template being applied (its own free parameters are the keys of
    /// `args`).
    pub base: TypeId,
    /// The substitution `TypeParamId → TypeId`, **sorted by the id** so two equal
    /// instantiations hash-cons to one node.
    pub args: Vec<(TypeParamId, TypeId)>,
}

/// A mapped-type modifier operator (`?`/`readonly`) — how the node adjusts a
/// property's optionality or readonly-ness (M26). `Keep` applies no change (the
/// homomorphic default, preserving the source property's flag); `Add` sets the flag
/// (`?`/`+?`, `readonly`/`+readonly`); `Remove` clears it (`-?`, `-readonly`).
/// Identity-bearing (folded into the mapped node's hash/eq).
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

/// Render an `f64` the way JavaScript's `String(n)` would for the common finite cases
/// — the value a numeric hole contributes to a constructed template literal (M27), and
/// the canonical form a `` `${number}` `` segment is validated against. Integers and
/// simple decimals match `String(n)` exactly (Rust's shortest round-trip Display);
/// negative zero normalizes to `"0"`, and the non-finite forms use the JS spellings.
/// Scientific/large-magnitude forms diverge from `String(n)` (out of the M27 subset —
/// documented, conservative).
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
    /// The optionality modifier (`?`/`+?`/`-?`), applied to each property's starting
    /// optional flag.
    pub optional_modifier: ModifierOp,
    /// The readonly modifier (`readonly`/`+readonly`/`-readonly`), applied to each
    /// property's starting readonly flag.
    pub readonly_modifier: ModifierOp,
}

#[cfg(test)]
mod tests {
    use super::number_to_string;

    /// M27 — JS-`String(n)`-faithful number rendering for the common finite cases:
    /// integers drop the decimal, simple decimals round-trip, and negative zero
    /// normalizes to `"0"`.
    #[test]
    fn number_to_string_matches_js_for_common_cases() {
        assert_eq!(number_to_string(1.0), "1");
        assert_eq!(number_to_string(42.0), "42");
        assert_eq!(number_to_string(3.5), "3.5");
        assert_eq!(number_to_string(0.0), "0");
        assert_eq!(number_to_string(-0.0), "0", "negative zero renders as 0");
        assert_eq!(number_to_string(0.5), "0.5");
    }
}
