// Backlog 103, the guard tier — the third panic site. A user `namespace Intl` resolves to the
// frozen namespace row, whose fragment list a delta may not extend (ADR-0011). The refusal is
// recorded at the declaration; it used to panic in the namespace binder.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean — namespace merging is legal.
// typokat refuses it, so the user's own member is not reachable through `Intl`; the TK2694
// over-report is ledgered in docs/reference/divergences.md under backlog 103.
namespace Intl { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  export interface B103Extra { // incomplete[bind/frozen-library-global/merge-refused]
    tag: string;
  }
}

declare const extra: Intl.B103Extra; // error[TK2694]
