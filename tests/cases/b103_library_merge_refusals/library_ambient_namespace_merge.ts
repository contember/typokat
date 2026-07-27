// Backlog 103, the guard tier. The ambient spelling of the namespace refusal — `declare
// namespace Intl` reaches the same frozen row as the non-ambient one, and panicked the same way.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean. typokat refuses the merge;
// ledgered in docs/reference/divergences.md under backlog 103.
declare namespace Intl { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  interface B103Ambient { // incomplete[bind/frozen-library-global/merge-refused]
    tag: string;
  }
}

declare const ambient: Intl.B103Ambient; // error[TK2694]
