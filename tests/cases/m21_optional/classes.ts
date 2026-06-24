// M21 — optional instance fields on classes (`name?: T`).
//
// A class with only public fields has a structural instance type, so optional fields
// behave exactly like optional members on an object type: absent is fine, reading
// yields `T | undefined`, a wrong-typed value errors. (`id` is initialized in the
// constructor so the declaration itself is clean under strict.)

class C {
  id: number;
  name?: string;
  constructor(id: number) {
    this.id = id;
  }
}

// Structural satisfaction of the instance type by an object literal.
const a: C = { id: 1 };            // ok — `name` is optional
const b: C = { id: 1, name: "x" }; // ok
const c: C = { id: 1, name: 5 };   // error[TK2322]
const d: C = { name: "x" };        // error[TK2741]: Property 'id' is missing in type

// Reading an optional field yields `string | undefined`; a required field is exact.
function readOpt(x: C): string { return x.name; } // error[TK2322]
function readReq(x: C): number { return x.id; }   // ok
