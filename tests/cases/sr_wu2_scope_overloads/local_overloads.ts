// WU2 / finding 6 — overload grouping only runs at module top level; inside a
// function body each signature-only declaration incorrectly fires TK2391
// "missing implementation" (false positive today), and calls resolve against the
// implementation signature only (so wrong-argument calls miss the tsc verdict
// and well-typed calls over-report on the widened implementation return type).
// This corpus pins the tsc-correct behavior. DISABLED at HEAD; enabling exposes
// the spurious TK2391 pair, the false TK2322 on the valid calls, and the wrong
// code on the bad call. Cross-checked vs tsc 6.0.3 --strict.

function outer(): void {
  // A valid local overload set: no TK2391, and calls select declared overloads.
  function ov(x: number): number;
  function ov(x: string): string;
  function ov(x: number | string): number | string {
    return x;
  }

  // controls: each call resolves to the matching declared overload's return
  // type, so these are clean (tsc: `number` / `string`). At HEAD they wrongly
  // report TK2322 because the call resolves against the implementation's
  // `number | string` return.
  const okNum: number = ov(1);
  const okStr: string = ov("a");

  // witness: no declared overload accepts `boolean` — tsc: TS2769.
  ov(true); // error[TK2769]: No overload matches this call
}
