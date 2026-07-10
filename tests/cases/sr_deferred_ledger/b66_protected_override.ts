// Deferred ledger / backlog 66 — override compatibility (TK2416) is checked
// public↔public only; an incompatible `protected`-over-`protected` override is
// skipped, so a genuine tsc TS2416 is DROPPED. This corpus stays DISABLED beyond
// this sprint (until backlog 66 runs the variance query on protected pairs while
// bypassing the nominal same-declaration guard). Cross-checked vs tsc 6.0.3
// --strict. Asserted code-only (method-signature target).

class Base {
  protected m(x: string): void {}
}

// witness (dropped error): incompatible protected override — tsc: TS2416 on `m`.
class Derived extends Base {
  protected m(x: number): void {} // error[TK2416]
}

// --- control: a compatible protected override (same signature) stays clean in
// both — the nominal guard must not reject a legal protected redeclaration. ---
class Legit extends Base {
  protected m(x: string): void {}
}
