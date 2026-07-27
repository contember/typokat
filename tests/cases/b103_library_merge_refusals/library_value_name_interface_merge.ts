// Backlog 103, the guard tier — the second panic site. `console` is a library VALUE with no
// type group of its own, so the fresh group allocates fine and it is the frozen SYMBOL row that
// cannot take the type slot. That reached a separate `.expect` (`resolved symbol exists`) and is
// now recorded like the rest.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean — a type and a value may share
// a name — and gives the interface member type `number`, so line 19 would be TS2322 there.
// typokat refuses the write, the annotation degrades to an error type, and every member read
// goes unchecked. That under-report is ledgered in docs/reference/divergences.md under backlog
// 103; the declaration's own incomplete record is what keeps the run honest (exit 3).
interface console { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  b103Probe: number;
}

declare const probeHolder: console;

// All three reads are silent — including a member that was never declared, which is how the
// corpus pins that the annotation degraded rather than resolving to the user's interface.
const probe: number = probeHolder.b103Probe;
const wrongProbe: string = probeHolder.b103Probe;
const neverDeclared: string = probeHolder.b103NotDeclaredAnywhere;

// The library VALUE slot is untouched by the refused type write: `console` keeps its own type.
const consoleValue: string = console; // error[TK2322]
