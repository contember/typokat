// Backlog 103, the guard tier. A script-top-level `interface Array<T>` merges into a
// library-owned type group. ADR-0011 forbids appending to a frozen group, so the merge is
// refused and recorded at the declaration — it used to panic the binder outright.
// Making the merge WORK is backlog 103's correctness tier, not this one.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` accepts the merge and reports only
// the deliberate TS2322 witness. typokat refuses it, so both reads become TK2339 instead;
// that over-report is ledgered in docs/reference/divergences.md under backlog 103.
interface Array<T> { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  b103First(): T;
}

const firstElement: number = [1, 2, 3].b103First(); // error[TK2339]: Property 'b103First' does not exist
const wrongFirstElement: string = [1, 2, 3].b103First(); // error[TK2339]: Property 'b103First' does not exist
