// Backlog 103 correctness tier, cross-slot collision. `console` is a library value with no type
// group of its own; the private epoch must add the interface's type slot without disturbing the
// existing value slot.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` accepts the declaration because a type
// and a value may share a name. It reports the deliberate TS2322/TS2339/TS2322 witnesses below.
interface console {
  b103Probe: number;
}

declare const probeHolder: console;

const probe: number = probeHolder.b103Probe;
const wrongProbe: string = probeHolder.b103Probe; // error[TK2322]: Type 'number' is not assignable to type 'string'
const neverDeclared: string = probeHolder.b103NotDeclaredAnywhere; // error[TK2339]

// The library value slot is untouched by the type-slot merge: `console` keeps its own type.
const consoleValue: string = console; // error[TK2322]
