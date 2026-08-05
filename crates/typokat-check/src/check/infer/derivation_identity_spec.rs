use super::*;
use crate::types::repr::{
    MappedType, ModifierOp, ObjectType, PropertyType, TupleRestType, TupleType, TypeParamId,
};
use std::cell::Cell;

struct IdentityNormalization;

impl RelationNormalization for IdentityNormalization {
    fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
        Ok(ty)
    }
}

fn object(interner: &mut Interner, value: TypeId) -> TypeId {
    interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", value)],
        ..Default::default()
    })
}

fn substitute_one(
    interner: &mut Interner,
    template: TypeId,
    parameter: TypeParamId,
    argument: DerivedType,
) -> DerivedType {
    crate::types::substitute_derived(
        interner,
        template,
        &FxHashMap::from_iter([(parameter, argument)]),
    )
}

fn promote_outer(interner: &mut Interner, reserved: TypeId, inner: TypeId) {
    interner.fill_object(
        reserved,
        ObjectType {
            properties: vec![PropertyType::public("value", inner)],
            ..Default::default()
        },
    );
    assert_eq!(
        interner
            .promote_caller_certified_acyclic_reserved_object(reserved)
            .expect("outer object is acyclic"),
        reserved
    );
}

fn assert_occurrence_identity_preserves_inference(reverse: bool) {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // Expanding instantiations reserve their parent rows before materializing children.
    let reserved_source = interner.reserve_object();
    let reserved_target = interner.reserve_object();

    let a_id = TypeParamId(92_001);
    let b_id = TypeParamId(92_002);
    let c_id = TypeParamId(92_003);
    let d_id = TypeParamId(92_004);
    let inferred_id = TypeParamId(92_005);
    let a_param = interner.intern_type_param(a_id, "P");
    let b_param = interner.intern_type_param(b_id, "Q");
    let c_param = interner.intern_type_param(c_id, "R");
    let d_param = interner.intern_type_param(d_id, "S");
    let inferred = interner.intern_type_param(inferred_id, "T");
    let a = object(&mut interner, a_param);
    let b = object(&mut interner, b_param);
    let c = object(&mut interner, c_param);
    let d = object(&mut interner, d_param);
    assert_ne!(a, b, "A<P> and B<Q> are distinct templates");
    assert_ne!(c, d, "C<R> and D<S> are distinct templates");

    let (first_source, second_source, outer_source_template, outer_source_param) = if reverse {
        let first = substitute_one(&mut interner, b, b_id, DerivedType::plain(wk.string));
        let second = substitute_one(&mut interner, a, a_id, DerivedType::plain(wk.string));
        (first, second, a, a_id)
    } else {
        let first = substitute_one(&mut interner, a, a_id, DerivedType::plain(wk.string));
        let second = substitute_one(&mut interner, b, b_id, DerivedType::plain(wk.string));
        (first, second, b, b_id)
    };
    assert_eq!(
        first_source.ty, second_source.ty,
        "the source results hash-cons"
    );
    promote_outer(&mut interner, reserved_source, first_source.ty);
    let outer_source = substitute_one(
        &mut interner,
        outer_source_template,
        outer_source_param,
        first_source,
    );
    assert_eq!(outer_source.ty, reserved_source);
    assert!(
        outer_source.ty < first_source.ty,
        "the parent row precedes its child"
    );

    let (first_target, second_target, outer_target_template, outer_target_param) = if reverse {
        let first = substitute_one(&mut interner, d, d_id, DerivedType::plain(inferred));
        let second = substitute_one(&mut interner, c, c_id, DerivedType::plain(inferred));
        (first, second, c, c_id)
    } else {
        let first = substitute_one(&mut interner, c, c_id, DerivedType::plain(inferred));
        let second = substitute_one(&mut interner, d, d_id, DerivedType::plain(inferred));
        (first, second, d, d_id)
    };
    assert_eq!(
        first_target.ty, second_target.ty,
        "the target results hash-cons"
    );
    promote_outer(&mut interner, reserved_target, first_target.ty);
    let outer_target = substitute_one(
        &mut interner,
        outer_target_template,
        outer_target_param,
        first_target,
    );
    assert_eq!(outer_target.ty, reserved_target);
    assert!(
        outer_target.ty < first_target.ty,
        "the parent row precedes its child"
    );

    // TypeScript oracle: `f(null as A<B<string>>)` infers `string` for
    // `declare function f<T>(value: C<D<T>>): T` because all wrappers are `{ value: X }`.
    let InferenceAttempt::Complete(candidates) = infer_from_derived_types_for_query(
        &mut interner,
        outer_source,
        outer_target,
        &IdentityNormalization,
    ) else {
        panic!("finite structural inference must complete")
    };

    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..]),
        "another producer's origin must not fabricate recursive occurrences"
    );
}

#[test]
fn occurrence_identity_survives_a_then_b_and_c_then_d() {
    assert_occurrence_identity_preserves_inference(false);
}

#[test]
fn occurrence_identity_survives_b_then_a_and_d_then_c() {
    assert_occurrence_identity_preserves_inference(true);
}

struct ExactOccurrenceWitness {
    expected: DerivedType,
    observations: Cell<usize>,
}

impl RelationNormalization for ExactOccurrenceWitness {
    fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
        Ok(ty)
    }

    fn normalize_derived(
        &self,
        _store: &Store,
        derived: DerivedType,
    ) -> Result<DerivedType, Exhaustion> {
        if derived.ty == self.expected.ty {
            assert_eq!(
                derived.derivation, self.expected.derivation,
                "structural traversal must retain the exact child occurrence"
            );
            self.observations
                .set(self.observations.get().saturating_add(1));
        }
        Ok(derived)
    }
}

fn equal_semantic_occurrences(interner: &mut Interner) -> (DerivedType, DerivedType) {
    let wk = interner.well_known();
    let first_id = TypeParamId(92_101);
    let second_id = TypeParamId(92_102);
    let first_param = interner.intern_type_param(first_id, "First");
    let second_param = interner.intern_type_param(second_id, "Second");
    let first_template = object(interner, first_param);
    let second_template = object(interner, second_param);
    let first = substitute_one(
        interner,
        first_template,
        first_id,
        DerivedType::plain(wk.string),
    );
    let second = substitute_one(
        interner,
        second_template,
        second_id,
        DerivedType::plain(wk.string),
    );
    assert_eq!(first.ty, second.ty, "the occurrence witnesses hash-cons");
    assert_ne!(
        first.derivation, second.derivation,
        "the occurrence witnesses retain distinct producers"
    );
    (first, second)
}

fn assert_exact_occurrence_reaches_inference(
    interner: &mut Interner,
    source: DerivedType,
    target: TypeId,
    expected: DerivedType,
    inferred_id: TypeParamId,
) {
    let witness = ExactOccurrenceWitness {
        expected,
        observations: Cell::new(0),
    };
    let InferenceAttempt::Complete(candidates) =
        infer_from_derived_types_for_query(interner, source, DerivedType::plain(target), &witness)
    else {
        panic!("finite occurrence inference must complete")
    };
    assert!(
        witness.observations.get() > 0,
        "the traversal must expose its occurrence-bearing child"
    );
    assert_eq!(
        candidates.get(&inferred_id).map(Vec::as_slice),
        Some(&[expected.ty][..])
    );
}

fn tuple_occurrence_case() -> (Interner, DerivedType, DerivedType, TypeId, TypeParamId) {
    let mut interner = Interner::with_intrinsics();
    let (_, expected) = equal_semantic_occurrences(&mut interner);
    let container_id = TypeParamId(92_103);
    let container_param = interner.intern_type_param(container_id, "Container");
    let source_template = interner.intern_tuple(vec![container_param]);
    let source = substitute_one(&mut interner, source_template, container_id, expected);
    let inferred_id = TypeParamId(92_104);
    let inferred = interner.intern_type_param(inferred_id, "T");
    (interner, source, expected, inferred, inferred_id)
}

#[test]
fn positional_tuple_traversal_preserves_exact_occurrence() {
    let (mut interner, source, expected, inferred, inferred_id) = tuple_occurrence_case();
    let target = interner.intern_tuple(vec![inferred]);
    assert_exact_occurrence_reaches_inference(&mut interner, source, target, expected, inferred_id);
}

#[test]
fn tuple_rest_traversal_preserves_exact_occurrence() {
    let (mut interner, source, expected, inferred, inferred_id) = tuple_occurrence_case();
    let rest = interner.intern_array(inferred);
    let target = interner.intern_tuple_type(TupleType::with_rest(
        Vec::new(),
        TupleRestType::new(0, rest),
    ));
    assert_exact_occurrence_reaches_inference(&mut interner, source, target, expected, inferred_id);
}

#[test]
fn tuple_to_array_traversal_preserves_exact_occurrence() {
    let (mut interner, source, expected, inferred, inferred_id) = tuple_occurrence_case();
    let target = interner.intern_array(inferred);
    assert_exact_occurrence_reaches_inference(&mut interner, source, target, expected, inferred_id);
}

#[test]
fn array_to_tuple_traversal_preserves_exact_occurrence() {
    let mut interner = Interner::with_intrinsics();
    let (_, expected) = equal_semantic_occurrences(&mut interner);
    let container_id = TypeParamId(92_105);
    let container_param = interner.intern_type_param(container_id, "Container");
    let source_template = interner.intern_array(container_param);
    let source = substitute_one(&mut interner, source_template, container_id, expected);
    let inferred_id = TypeParamId(92_106);
    let inferred = interner.intern_type_param(inferred_id, "T");
    let target = interner.intern_tuple(vec![inferred]);
    assert_exact_occurrence_reaches_inference(&mut interner, source, target, expected, inferred_id);
}

#[test]
fn identity_mapped_target_traversal_preserves_exact_occurrence() {
    let mut interner = Interner::with_intrinsics();
    let (_, source) = equal_semantic_occurrences(&mut interner);
    let inferred_id = TypeParamId(92_107);
    let inferred = interner.intern_type_param(inferred_id, "T");
    let mapped_value = interner.intern_mapped_value();
    let target = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: inferred,
        value_template: mapped_value,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    assert_exact_occurrence_reaches_inference(&mut interner, source, target, source, inferred_id);
}
