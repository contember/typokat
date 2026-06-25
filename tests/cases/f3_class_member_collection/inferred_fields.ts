// F3 / backlog 01 — class fields with an INITIALIZER but NO annotation become
// real members whose type is inferred from the initializer. Before this fix an
// un-annotated field was dropped, so every access over-reported TK2339.
//
// Widening: a non-readonly initialized field widens its literal initializer
// (`g = 2` → `number`), matching tsc — witnessed by `d.g = 5` checking CLEAN
// (a non-widened `2` would reject `5`). Cross-checked against tsc 6.0.3.

class D {
  f = () => 1;
  g = 2;
  s = "hi";
}

const d = new D();
const n: number = d.g;   // ok — g inferred + widened to number
const r: number = d.f(); // ok — f inferred as () => number
const t: string = d.s;   // ok — s inferred as string
d.g = 5;                 // ok — g is number (widened), 5 assignable
d.g = "x";               // error[TK2322]: not assignable to type 'number'
const bad: string = d.g; // error[TK2322]: not assignable to type 'string'
