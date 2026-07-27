// Backlog 103, the guard tier. The DOM shape of the same refusal — the one users actually
// write. `interface Window` is the canonical augmentation idiom and it used to panic the binder.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean — the merge is legal. typokat
// refuses it, so the augmented member stays off the library `Window`; that over-report is
// ledgered in docs/reference/divergences.md under backlog 103.
//
// The reads go through a `Window` ANNOTATION, not the global `window` value: on the library base
// `window` is not modelled yet and every read through it is silent, which would make the
// witnesses vacuous. That gap is backlog 14's, not this boundary's.
interface Window { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  b103Flag: boolean;
}

declare const view: Window;
const flag: boolean = view.b103Flag; // error[TK2339]: Property 'b103Flag' does not exist
// The library surface survived the refused write intact.
const width: number = view.innerWidth;
const wrongWidth: string = view.innerWidth; // error[TK2322]
