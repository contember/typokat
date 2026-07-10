// Surface-accounting spec (backlog 75). ENABLED by WU5 (review F3): class heritage type
// arguments and implements clauses are RECORD-ONLY accounted — the checker does not
// lower/check them (no new semantics), but they can no longer exit clean. See
// tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `resolve_base_class` reads only the plain-identifier super class —
// `super_type_arguments` were never lowered (`extends B<typeof Missing>` was
// false-clean; tsc 6.0.3 --strict: TS2304), and the `implements` clause was entirely
// unprocessed (`implements NoSuchInterface` was false-clean; tsc: TS2304). The record
// fires for EVERY `extends B<...>` — even a well-formed one — because generic-base
// composition itself is deferred (divergences.md `classes/override-generic-base`).

class B<T> {
  v!: T;
}

// INCOMPLETE (F3a): extends type arguments are not lowered — the unresolved name inside
// is never reported.
class C extends B<typeof Missing> {} // incomplete[class/class-heritage/type-arguments]

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
