// Surface-accounting spec (backlog 75). ENABLED by WU5 (review F3): class heritage type
// arguments and implements clauses are RECORD-ONLY accounted for composition. WU2
// additionally traverses heritage argument syntax without publishing a partial base. See
// tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `resolve_base_class` reads only the plain-identifier super class —
// generic-base composition remains deferred (divergences.md
// `classes/override-generic-base`), while nested unsupported syntax retains its own
// record. The `implements` clause remains entirely unprocessed.

class B<T> {
  v!: T;
}

// INCOMPLETE (F3a): generic-base composition is deferred; the nested type query is
// traversed and keeps its canonical record.
class C extends B<typeof Missing> {} // incomplete[annotation-lower/type-query/typeof] | incomplete[class/class-heritage/type-arguments]

// INCOMPLETE (F3a): a well-formed instantiation is unaccounted too (composition ignores it).
class E extends B<number> {} // incomplete[class/class-heritage/type-arguments]

// INCOMPLETE (F3b): the implements clause is not processed — the unresolved interface
// name is never reported.
class D implements NoSuchInterface {} // incomplete[class/implements-clause/self]

// CONTROL (supported): a non-generic plain-identifier base composes with no record.
class Base {
  n: number = 1;
}
class F extends Base {}
let f: number = new F().n;
