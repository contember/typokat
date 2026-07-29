// Backlog 103 correctness tier. `interface Window` is the canonical DOM augmentation idiom and
// must merge into the library-owned group through the private collision epoch.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` accepts the merge and reports only the
// deliberate TS2322 witness on the final line.
//
// The reads go through a `Window` ANNOTATION, not the global `window` value: on the library base
// `window` is not modelled yet and every read through it is silent, which would make the
// witnesses vacuous. That gap is backlog 14's, not this boundary's.
interface Window {
  b103Flag: boolean;
}

declare const view: Window;
const flag: boolean = view.b103Flag;
// The library surface survives the augmentation intact.
const width: number = view.innerWidth;
const wrongWidth: string = view.innerWidth; // error[TK2322]
