use super::*;
use crate::types::repr::{ObjectType, PropertyType, TypeParamId};

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
    argument: TypeId,
) -> TypeId {
    substitute(
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
        let first = substitute_one(&mut interner, b, b_id, wk.string);
        let second = substitute_one(&mut interner, a, a_id, wk.string);
        (first, second, a, a_id)
    } else {
        let first = substitute_one(&mut interner, a, a_id, wk.string);
        let second = substitute_one(&mut interner, b, b_id, wk.string);
        (first, second, b, b_id)
    };
    assert_eq!(first_source, second_source, "the source results hash-cons");
    promote_outer(&mut interner, reserved_source, first_source);
    let outer_source = substitute_one(
        &mut interner,
        outer_source_template,
        outer_source_param,
        first_source,
    );
    assert_eq!(outer_source, reserved_source);
    assert!(outer_source < first_source, "the parent row precedes its child");

    let (first_target, second_target, outer_target_template, outer_target_param) = if reverse {
        let first = substitute_one(&mut interner, d, d_id, inferred);
        let second = substitute_one(&mut interner, c, c_id, inferred);
        (first, second, c, c_id)
    } else {
        let first = substitute_one(&mut interner, c, c_id, inferred);
        let second = substitute_one(&mut interner, d, d_id, inferred);
        (first, second, d, d_id)
    };
    assert_eq!(first_target, second_target, "the target results hash-cons");
    promote_outer(&mut interner, reserved_target, first_target);
    let outer_target = substitute_one(
        &mut interner,
        outer_target_template,
        outer_target_param,
        first_target,
    );
    assert_eq!(outer_target, reserved_target);
    assert!(outer_target < first_target, "the parent row precedes its child");

    // TypeScript oracle: `f(null as A<B<string>>)` infers `string` for
    // `declare function f<T>(value: C<D<T>>): T` because all wrappers are `{ value: X }`.
    let mut candidates = Candidates::default();
    assert!(
        matches!(
            infer_from_types_for_query(
                &mut interner,
                outer_source,
                outer_target,
                &mut candidates,
                &IdentityNormalization,
            ),
            DemandOutcome::Ready(())
        ),
        "finite structural inference must complete"
    );

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
