// WU2 / finding 5 — every `case`/`default` clause binds into the switch's PARENT
// scope instead of one switch-local lexical scope (bind.rs:369), so a `let`/
// `const` declared in a clause incorrectly resolves AFTER the switch ends. The
// witness is a MISSING TK2304 on the after-switch use. DISABLED at HEAD;
// enabling exposes it. Cross-checked vs tsc 6.0.3 --strict.
//
// Scope note (per the sprint): this corpus pins ONLY the switch-local boundary
// and the after-switch TK2304. Duplicate-declaration diagnostics (TK2451 /
// TK2300, including cross-case redeclaration) stay owned by backlog 18 and are
// NOT asserted here. tsc's TS2454 "used before being assigned" is a
// definite-assignment check typokat does not emit (deferred — see divergences).

function afterSwitchLeak(x: number): void {
  switch (x) {
    case 1:
      let y = 1;
      break;
    case 2:
      // control: cross-clause visibility — an earlier clause's `let` still
      // resolves in a later clause (no TK2304). tsc additionally reports TS2454
      // here (deferred definite-assignment check; typokat stays clean).
      const seen: number = y;
      break;
  }
  // witness: after the switch, `y` must NOT be visible — tsc: TS2304.
  const after: number = y; // error[TK2304]: Cannot find name 'y'
}

function explicitBlockControl(x: number): void {
  switch (x) {
    case 3: {
      // control: an explicit `{ }` block inside a clause keeps its own nested
      // scope; `inner` is not visible after the switch. This already holds at
      // HEAD and must stay true after the fix.
      let inner = 5;
      break;
    }
  }
  const q: number = inner; // error[TK2304]: Cannot find name 'inner'
}
