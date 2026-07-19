# Distribution guards must respect blocked binders

`Substitution::apply_conditional` (distributive guard) and `apply_mapped` (homomorphic guard)
look up the check/key parameter in the substitution map without consulting `blocked`, so a
parameter shadowed by a same-id generic binder can still trigger distribution and capture the
outer argument (an over-wrap; `tsc --strict` does not error on the witness shape). The
param-relevant prefilter already resolves the fully-shadowed case toward tsc by proving the
subtree identity; the residual misfire needs the guards to filter `blocked` the way the
`TypeParam` arm does.

Witness (adversarial-review repro, should become a conformance fixture with the fix):

```ts
interface Box { <U>(x: U, y: U extends string ? 1 : 2): Box; }
declare const b: Box;
declare const u: "a" | "b";
const r = b(u, 1);
const ok: Box = r; // tsc: clean; typokat HEAD-before-prefilter: spurious TK2322
```

Spec-first: pin the fixture (clean expectation) plus a control where distribution legitimately
fires, then make both guards skip blocked ids. Ledger entry:
`substitution/distribution-guard-ignores-blocked` in
[divergences.md](../reference/divergences.md).
