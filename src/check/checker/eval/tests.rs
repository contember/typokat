use super::*;
use crate::check::checker::eval::keyof::keyof_of_object;
use crate::types::repr::{ObjectType, PropertyType};

fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

/// Witness (architecture §7.2 item b): a ~10 000-deep nested `{ v: … }` type
/// evaluated by an `Unwrap`-style recursive conditional resolves to the innermost
/// type **without overflowing the native stack**. Built programmatically via the
/// interner (a parsed fixture would stress the parser instead), and run with a raised
/// budget so the work-stack — not the step budget — is what proves termination.
#[test]
fn deep_recursive_unwrap_does_not_overflow_the_native_stack() {
    const DEPTH: usize = 10_000;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // The recursive template: `type Unwrap<T> = T extends { v: infer U } ? Unwrap<U> : T`.
    // T = TypeParamId(0); the true branch is a lazy self-instantiation carrying the
    // infer binder as the argument.
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let infer0 = interner.intern_infer(0);
    let extends = interner.intern_object(ObjectType {
        properties: vec![prop("v", infer0)],
        ..Default::default()
    });
    let template = interner.reserve_conditional();
    let recur = interner.intern_instantiation(template, vec![(TypeParamId(0), infer0)]);
    interner.fill_conditional(
        template,
        ConditionalType {
            check: t,
            extends_ty: extends,
            true_branch: recur,
            false_branch: t,
            infer_count: 1,
            distributive: true,
            poisoned: false,
        },
    );

    // Build the 10 000-deep check type `{ v: { v: … { v: number } … } }`, innermost
    // out (iteratively — no recursion here either).
    let mut deep = wk.number;
    for _ in 0..DEPTH {
        deep = interner.intern_object(ObjectType {
            properties: vec![prop("v", deep)],
            ..Default::default()
        });
    }

    // `Unwrap<deep>` — evaluate with a budget above the depth so termination is the
    // work-stack's doing, not the budget's.
    let root = interner.intern_instantiation(template, vec![(TypeParamId(0), deep)]);
    let mut next_type_param: u32 = 1;
    let mut memo = FxHashMap::default();
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        (DEPTH as u32) + 1000,
    );
    let result = ev.evaluate(root);
    assert!(!ev.exhausted, "the raised budget must not be exhausted");
    assert_eq!(
        result, wk.number,
        "Unwrap fully descends to the innermost `number`"
    );
}

/// A terminating shallow `Unwrap` resolves, and its memo is populated (a repeat
/// evaluation is a cache hit).
#[test]
fn shallow_unwrap_resolves_and_memoizes() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let infer0 = interner.intern_infer(0);
    let extends = interner.intern_object(ObjectType {
        properties: vec![prop("v", infer0)],
        ..Default::default()
    });
    let template = interner.reserve_conditional();
    let recur = interner.intern_instantiation(template, vec![(TypeParamId(0), infer0)]);
    interner.fill_conditional(
        template,
        ConditionalType {
            check: t,
            extends_ty: extends,
            true_branch: recur,
            false_branch: t,
            infer_count: 1,
            distributive: true,
            poisoned: false,
        },
    );
    // `{ v: { v: number } }`.
    let inner = interner.intern_object(ObjectType {
        properties: vec![prop("v", wk.number)],
        ..Default::default()
    });
    let outer = interner.intern_object(ObjectType {
        properties: vec![prop("v", inner)],
        ..Default::default()
    });
    let root = interner.intern_instantiation(template, vec![(TypeParamId(0), outer)]);

    let mut next_type_param: u32 = 1;
    let mut memo = FxHashMap::default();
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        DEFAULT_STEP_BUDGET,
    );
    assert_eq!(ev.evaluate(root), wk.number);
    assert!(memo.contains_key(&root), "the root evaluation is memoized");
}

/// A **poisoned** conditional (cross-binder nested infer — backlog 26 stopgap) NEVER
/// evaluates, even with a fully concrete check: the evaluator returns the node
/// as-is (both directly and through an instantiation of a poisoned template), so it
/// stays a deferred node under the conservative relation rules.
#[test]
fn poisoned_conditional_never_evaluates() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let s_lit = interner.intern_literal(LiteralValue::String("s".into()));
    let n_lit = interner.intern_literal(LiteralValue::String("n".into()));

    // A concrete-check poisoned node: `string extends string ? "s" : "n"` — would
    // resolve to "s" if evaluation were allowed.
    let poisoned = interner.intern_conditional(ConditionalType {
        check: wk.string,
        extends_ty: wk.string,
        true_branch: s_lit,
        false_branch: n_lit,
        infer_count: 0,
        distributive: false,
        poisoned: true,
    });
    let mut next_type_param: u32 = 0;
    let mut memo = FxHashMap::default();
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        DEFAULT_STEP_BUDGET,
    );
    assert_eq!(
        ev.evaluate(poisoned),
        poisoned,
        "a poisoned conditional must be returned as-is, never evaluated"
    );
    assert!(!ev.exhausted);
    drop(ev);

    // Through an instantiation of a poisoned distributive template: the expansion
    // must NOT distribute (a poisoned base is treated as non-distributive) — the
    // result is the substituted, still-poisoned node, unevaluated.
    let t = interner.intern_type_param(TypeParamId(900), "T");
    let template = interner.intern_conditional(ConditionalType {
        check: t,
        extends_ty: wk.string,
        true_branch: s_lit,
        false_branch: n_lit,
        infer_count: 0,
        distributive: true,
        poisoned: true,
    });
    let union = interner.union(vec![wk.string, wk.number]);
    let root = interner.intern_instantiation(template, vec![(TypeParamId(900), union)]);
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        DEFAULT_STEP_BUDGET,
    );
    let result = ev.evaluate(root);
    drop(ev);
    let out = interner
        .store()
        .conditional_type(result)
        .copied()
        .expect("the instantiation must resolve to a (deferred) conditional node");
    assert!(out.poisoned, "the substituted node stays poisoned");
    assert_eq!(out.check, union, "substituted once, never distributed");
}

/// A genuinely-infinite alias (`type Inf<T> = T extends {} ? Inf<{ v: T }> : never`)
/// trips the step budget rather than looping, setting `exhausted`.
#[test]
fn runaway_growth_exhausts_the_budget() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let empty = interner.intern_object(ObjectType::default());
    let template = interner.reserve_conditional();
    // The true branch wraps the check type: `Inf<{ v: T }>`.
    let wrapped = interner.intern_object(ObjectType {
        properties: vec![prop("v", t)],
        ..Default::default()
    });
    let recur = interner.intern_instantiation(template, vec![(TypeParamId(0), wrapped)]);
    interner.fill_conditional(
        template,
        ConditionalType {
            check: t,
            extends_ty: empty,
            true_branch: recur,
            false_branch: wk.never,
            infer_count: 0,
            distributive: true,
            poisoned: false,
        },
    );
    let root = interner.intern_instantiation(template, vec![(TypeParamId(0), empty)]);

    let mut next_type_param: u32 = 1;
    let mut memo = FxHashMap::default();
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        DEFAULT_STEP_BUDGET,
    );
    let _ = ev.evaluate(root);
    assert!(ev.exhausted, "a runaway alias must exhaust the step budget");
}

/// M26 — a homomorphic identity mapped type `{ [K in keyof T]: T[K] }` over a
/// concrete source evaluates to the source's shape (per-property `T[K]` = the source
/// property's type), and its result is memoized.
fn eval(
    interner: &mut Interner,
    next: &mut u32,
    memo: &mut FxHashMap<TypeId, TypeId>,
    ty: TypeId,
) -> TypeId {
    let mut ev = ConditionalEvaluator::new(interner, next, memo, DEFAULT_STEP_BUDGET);
    ev.evaluate(ty)
}

#[test]
fn mapped_identity_evaluates_to_source_shape() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    // Concrete source `{ a: number; b: string }`.
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    let placeholder = interner.intern_mapped_value();
    let ident = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: source,
        value_template: placeholder,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let mut next = 0u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, ident);
    assert_eq!(
        result, source,
        "an identity map over a concrete source yields the source shape"
    );
    assert!(
        memo.contains_key(&ident),
        "the mapped evaluation is memoized"
    );
}

/// M26 — modifier arithmetic: `readonly` (Add) sets every result property readonly;
/// `?` (Add) makes every property optional; a `MappedValue | null` template unions
/// `null` into each value.
#[test]
fn mapped_modifiers_and_value_transform_apply() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    let placeholder = interner.intern_mapped_value();
    // `{ readonly [K in keyof T]?: T[K] | null }`.
    let value_template = interner.union(vec![placeholder, wk.null]);
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: source,
        value_template,
        modifiers_source: None,
        optional_modifier: ModifierOp::Add,
        readonly_modifier: ModifierOp::Add,
    });
    let mut next = 0u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, mapped);

    let a = interner
        .store()
        .object_type(result)
        .and_then(|o| o.property("a"))
        .expect("property a survives")
        .clone();
    assert!(a.readonly, "readonly (Add) makes the property readonly");
    assert!(a.optional, "? (Add) makes the property optional");
    // Effective type is `number | null | undefined` (value `number | null`, plus the
    // optional `| undefined` baked in).
    let expected = interner.union(vec![wk.number, wk.null, wk.undefined]);
    assert_eq!(
        a.ty, expected,
        "value template `T[K] | null` + optional `| undefined`"
    );
}

/// M27 — template construction: all-literal holes **collapse** (`` `a-${"b"}` `` →
/// `"a-b"`), a union hole distributes to the cartesian-product union, `boolean`
/// expands to `"false" | "true"`, a `never` hole short-circuits to `never`, and a
/// number literal stringifies.
#[test]
fn template_construction_collapses_and_distributes() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut next = 0u32;
    let mut memo = FxHashMap::default();

    let s = |interner: &mut Interner, v: &str| {
        interner.intern_literal(LiteralValue::String(v.to_string()))
    };
    let template = |interner: &mut Interner, texts: &[&str], holes: Vec<TypeId>| {
        interner.intern_template(TemplateType {
            texts: texts.iter().map(|t| t.to_string()).collect(),
            holes,
        })
    };

    // `` `a-${"b"}` `` → "a-b".
    let b = s(&mut interner, "b");
    let one = template(&mut interner, &["a-", ""], vec![b]);
    let expect = s(&mut interner, "a-b");
    assert_eq!(eval(&mut interner, &mut next, &mut memo, one), expect);

    // `` `${"a"|"b"}-${"1"|"2"}` `` → "a-1" | "a-2" | "b-1" | "b-2".
    let a = s(&mut interner, "a");
    let b = s(&mut interner, "b");
    let d1 = s(&mut interner, "1");
    let d2 = s(&mut interner, "2");
    let ab = interner.union(vec![a, b]);
    let d12 = interner.union(vec![d1, d2]);
    let two = template(&mut interner, &["", "-", ""], vec![ab, d12]);
    let members: Vec<TypeId> = ["a-1", "a-2", "b-1", "b-2"]
        .into_iter()
        .map(|v| s(&mut interner, v))
        .collect();
    let expect = interner.union(members);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, two), expect);

    // `` `is:${boolean}` `` → "is:false" | "is:true".
    let bh = template(&mut interner, &["is:", ""], vec![wk.boolean]);
    let f = s(&mut interner, "is:false");
    let t = s(&mut interner, "is:true");
    let expect = interner.union(vec![f, t]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, bh), expect);

    // `` `x${never}` `` → never.
    let nh = template(&mut interner, &["x", ""], vec![wk.never]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, nh), wk.never);

    // `` `v${1|2}` `` → "v1" | "v2" (number stringify).
    let n1 = interner.intern_literal(LiteralValue::Number(1.0));
    let n2 = interner.intern_literal(LiteralValue::Number(2.0));
    let n12 = interner.union(vec![n1, n2]);
    let ver = template(&mut interner, &["v", ""], vec![n12]);
    let v1 = s(&mut interner, "v1");
    let v2 = s(&mut interner, "v2");
    let expect = interner.union(vec![v1, v2]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, ver), expect);
}

/// M27 — a template with a **non-literal** hole (a `string` intrinsic, or a free
/// declaration type parameter) stays a **symbolic** node; an **error-typed** hole
/// degrades the whole template to the error type (M22 cascade suppression).
#[test]
fn template_construction_keeps_symbolic_and_suppresses_error() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut next = 1u32;
    let mut memo = FxHashMap::default();

    let template = |interner: &mut Interner, hole: TypeId| {
        interner.intern_template(TemplateType {
            texts: vec!["tag:".to_string(), String::new()],
            holes: vec![hole],
        })
    };

    // `string` hole → symbolic pattern (unchanged). It memoizes to itself via the
    // SetMemo discipline (backlog 55) — idempotent, mirroring a conditional whose
    // concrete operands stay undecidable.
    let pattern = template(&mut interner, wk.string);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, pattern),
        pattern,
        "a `${{string}}` pattern stays symbolic"
    );
    assert_eq!(
        memo.get(&pattern).copied(),
        Some(pattern),
        "a symbolic template memoizes to itself (idempotent)"
    );

    // Free type parameter hole → deferred (symbolic).
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let deferred = template(&mut interner, t);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, deferred),
        deferred
    );

    // Error hole → error type (M22 cascade suppression).
    let err = template(&mut interner, wk.error);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, err),
        wk.error,
        "an error-typed hole degrades the template to the error type"
    );
}

/// M26 — a mapped type over a **free** declaration type parameter stays deferred: the
/// evaluator returns the node unchanged (related conservatively by the M25 model),
/// and it is NOT memoized.
#[test]
fn deferred_mapped_over_free_param_is_returned_unchanged() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let placeholder = interner.intern_mapped_value();
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: t, // a free parameter → deferred
        value_template: placeholder,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let mut next = 1u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, mapped);
    assert_eq!(
        result, mapped,
        "a deferred mapped type is returned unchanged"
    );
    assert!(
        !memo.contains_key(&mapped),
        "a deferred mapped type is not memoized"
    );
}

/// M28 — a **deferred `keyof`** over a free type parameter is returned unchanged
/// (and not memoized); once its operand is concrete (an object) it resolves
/// through the SHARED keyof computation to the key-literal union; an error
/// operand degrades to the error type; a concrete-but-non-object operand (a
/// primitive after substitution) stays a deferred node — never a permissive
/// fallback.
#[test]
fn deferred_keyof_defers_and_resolves_via_shared_computation() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut next = 1u32;
    let mut memo = FxHashMap::default();

    // Free operand: unchanged, un-memoized.
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let keyof_t = interner.intern_keyof(t);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, keyof_t), keyof_t);
    assert!(
        !memo.contains_key(&keyof_t),
        "a deferred keyof is not memoized"
    );

    // Concrete object operand: the key-literal union (same as the eager path).
    let obj = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    let keyof_obj = interner.intern_keyof(obj);
    let a = interner.intern_literal(LiteralValue::String("a".into()));
    let b = interner.intern_literal(LiteralValue::String("b".into()));
    let expect = interner.union(vec![a, b]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, keyof_obj), expect);
    let eager = keyof_of_object(&mut interner, obj).expect("object operand keys");
    assert_eq!(
        eager, expect,
        "single source of truth: eager == deferred result"
    );

    // Error operand: the error type (M22 cascade suppression).
    let keyof_err = interner.intern_keyof(wk.error);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, keyof_err),
        wk.error
    );

    // Concrete non-object operand: stays deferred (conservative, not permissive).
    let keyof_num = interner.intern_keyof(wk.number);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, keyof_num),
        keyof_num
    );
}

/// M28 string intrinsics: literals transform, unions distribute, and a
/// non-literal argument stays a symbolic instantiation.
#[test]
fn string_intrinsics_transform_distribute_and_stay_symbolic() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut next = 1u32;
    let mut memo = FxHashMap::default();
    let s_param = TypeParamId(0);

    let lit = |interner: &mut Interner, v: &str| {
        interner.intern_literal(LiteralValue::String(v.to_string()))
    };
    let apply = |interner: &mut Interner,
                 next: &mut u32,
                 memo: &mut FxHashMap<TypeId, TypeId>,
                 base: TypeId,
                 arg: TypeId| {
        let inst = interner.intern_instantiation(base, vec![(s_param, arg)]);
        let mut ev = ConditionalEvaluator::new(interner, next, memo, DEFAULT_STEP_BUDGET);
        ev.evaluate(inst)
    };

    // Literal transforms — the four kinds.
    let abc = lit(&mut interner, "abc");
    let big = lit(&mut interner, "ABC");
    let cases = [
        (wk.uppercase, abc, "ABC"),
        (wk.lowercase, big, "abc"),
        (wk.capitalize, abc, "Abc"),
        (wk.uncapitalize, big, "aBC"),
    ];
    for (base, arg, expect) in cases {
        let expect = lit(&mut interner, expect);
        assert_eq!(
            apply(&mut interner, &mut next, &mut memo, base, arg),
            expect
        );
    }
    // The empty string is unchanged (no first char to map).
    let empty = lit(&mut interner, "");
    assert_eq!(
        apply(&mut interner, &mut next, &mut memo, wk.capitalize, empty),
        empty
    );

    // A union argument distributes per member.
    let a = lit(&mut interner, "a");
    let b = lit(&mut interner, "b");
    let ab = interner.union(vec![a, b]);
    let big_a = lit(&mut interner, "A");
    let big_b = lit(&mut interner, "B");
    let expect = interner.union(vec![big_a, big_b]);
    assert_eq!(
        apply(&mut interner, &mut next, &mut memo, wk.uppercase, ab),
        expect
    );

    // A non-literal argument stays the symbolic (identical, hash-consed) node.
    let sym = interner.intern_instantiation(wk.uppercase, vec![(s_param, wk.string)]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, sym), sym);
}

/// M28 — a non-homomorphic map with a **modifiers source** (the `Pick` shape)
/// resolves each key against the source object: the property's value type
/// replaces the placeholder and its `?` flag survives (with the M21
/// `| undefined` baked in); a key the source lacks keeps the M26 behavior
/// (error-typed value, flags absent).
#[test]
fn modifiers_source_preserves_values_and_flags() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let placeholder = interner.intern_mapped_value();

    // Source `{ a: number; b?: string }` (M21 stores b as `string | undefined`).
    let str_or_undef = interner.union(vec![wk.string, wk.undefined]);
    let mut b = prop("b", str_or_undef);
    b.optional = true;
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), b],
        ..Default::default()
    });

    // `{ [P in "a" | "b" | "q"]: T[P] }` with modifiers source = the object.
    let a_key = interner.intern_literal(LiteralValue::String("a".into()));
    let b_key = interner.intern_literal(LiteralValue::String("b".into()));
    let q_key = interner.intern_literal(LiteralValue::String("q".into()));
    let keys = interner.union(vec![a_key, b_key, q_key]);
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: false,
        key_source: keys,
        value_template: placeholder,
        modifiers_source: Some(source),
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let mut next = 0u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, mapped);

    let props: Vec<PropertyType> = interner
        .store()
        .object_type(result)
        .expect("result is an object")
        .properties
        .clone();
    let get = |name: &str| {
        props
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("property {name} present"))
            .clone()
    };
    assert_eq!(get("a").ty, wk.number, "picked value type preserved");
    assert!(!get("a").optional);
    assert!(get("b").optional, "picked optionality preserved");
    assert_eq!(get("b").ty, str_or_undef);
    assert!(!get("q").optional, "a missing key keeps the M26 defaults");
    assert_eq!(get("q").ty, wk.error);
}

/// M26 — `-?` Required semantics (probed tsc 6.0.3, leader-arbitrated): over an
/// **optional** source member, `undefined` is stripped from the **evaluated** value
/// type — including a template-re-added `| undefined`; a result that is EXACTLY
/// `undefined` maps to `never`; a **non-optional** source member never strips
/// (template-added `undefined` is kept).
#[test]
fn required_strips_undefined_from_optional_source_values() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let placeholder = interner.intern_mapped_value();

    // Source `{ a: string | undefined; b?: string; u?: undefined }` — M21 stores an
    // optional member's effective type with `| undefined` baked in.
    let str_or_undef = interner.union(vec![wk.string, wk.undefined]);
    let mut b = prop("b", str_or_undef);
    b.optional = true;
    let mut u = prop("u", wk.undefined);
    u.optional = true;
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", str_or_undef), b, u],
        ..Default::default()
    });

    // `{ [K in keyof T]-?: T[K] | undefined }` — the template RE-ADDS `undefined`,
    // distinguishing a result-level strip from a source-level one.
    let template = interner.union(vec![placeholder, wk.undefined]);
    let req = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: source,
        value_template: template,
        modifiers_source: None,
        optional_modifier: ModifierOp::Remove,
        readonly_modifier: ModifierOp::Keep,
    });
    let mut next = 0u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, req);

    let props: Vec<PropertyType> = interner
        .store()
        .object_type(result)
        .expect("result is an object")
        .properties
        .clone();
    let get = |name: &str| {
        props
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("property {name} present"))
            .clone()
    };
    // Optional source `b`: undefined stripped from the whole RESULT (even the
    // template-added one) → exactly `string`, and required.
    let b_out = get("b");
    assert!(!b_out.optional, "-? clears optionality");
    assert_eq!(
        b_out.ty, wk.string,
        "undefined stripped from the evaluated value"
    );
    // Exactly-undefined optional source `u`: maps to `never` (leader-arbitrated
    // tsc probe m26_arb.ts — filtering `undefined` by not-undefined leaves nothing).
    assert_eq!(
        get("u").ty,
        wk.never,
        "an exactly-undefined value maps to never"
    );
    // NON-optional source `a`: never strips — keeps `string | undefined`.
    assert_eq!(
        get("a").ty,
        str_or_undef,
        "a non-optional source member keeps its undefined"
    );
}
