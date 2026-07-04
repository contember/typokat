// M23 — early exit narrows a discriminated union: the remaining code sees the
// other arm(s).

type A = { kind: "a"; a: number };
type B = { kind: "b"; b: string };

function discEarly(s: A | B) {
  if (s.kind === "a") return;
  const ok: B = s;
  const b: string = s.b;
}

function discEarlyNeg(s: A | B) {
  if (s.kind === "a") return;
  const bad: A = s; // error[TK2741]
}
