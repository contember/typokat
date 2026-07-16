// Surface-accounting spec (backlog 75). Local plain-identifier generic heritage is
// supported and traverses every argument; implements clauses remain record-only. See
// tests/cases/README.md ("Surface-accounting corpus").
//
// The separate `classes/override-generic-base` divergence concerns override validation
// inside generic inheritance, not composition of this represented heritage application.

class B<T> {
  v!: T;
}

// CONTROL: heritage arguments are traversed, so the unsupported nested type query keeps
// its canonical owner while the class never publishes a partial base.
class C extends B<typeof Missing> {} // incomplete[annotation-lower/type-query/typeof]

// CONTROL (supported): a well-formed generic application composes without a record.
class E extends B<number> {}

// INCOMPLETE (F3b): the implements clause is not processed — the unresolved interface
// name is never reported.
class D implements NoSuchInterface {} // incomplete[class/implements-clause/self]

// CONTROL (supported): a non-generic plain-identifier base composes with no record.
class Base {
  n: number = 1;
}
class F extends Base {}
let f: number = new F().n;
