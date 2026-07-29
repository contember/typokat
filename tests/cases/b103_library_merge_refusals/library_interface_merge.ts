// Backlog 103 correctness tier. A script-top-level `interface Array<T>` must merge into the
// library-owned type group through the private collision epoch.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` accepts the merge and reports only
// the deliberate TS2322 witness.
interface Array<T> {
  b103First(): T;
}

const firstElement: number = [1, 2, 3].b103First();
const wrongFirstElement: string = [1, 2, 3].b103First(); // error[TK2322]: Type 'number' is not assignable to type 'string'
