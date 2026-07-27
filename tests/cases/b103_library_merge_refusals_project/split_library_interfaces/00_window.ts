// Backlog 103, the guard tier — the split shape. Two files each merge into a DIFFERENT
// library-owned group; the refusal must be recorded per declaration, in both files.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` accepts both merges and reports only the
// deliberate TS2322 witness in 99_string.ts. typokat refuses both; ledgered in
// docs/reference/divergences.md under backlog 103.
interface Window { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  b103SplitFlag: boolean;
}
