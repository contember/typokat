// M22 — TK2304 ("Cannot find name") for an unresolved type reference in type position.
//
// Previously an unresolved type name silently degraded to the error type with no
// diagnostic. Now it reports TK2304 at the reference. The reference still resolves to
// the error type (any-like), so it SUPPRESSES any cascade: `const a: Foo = 5` reports
// only TK2304 — never also a TK2322 for `5` vs `Foo`.

const a: Foo = 5;                     // error[TK2304]: Cannot find name 'Foo'

function f(x: Bar): Baz { return x; } // error[TK2304]: Cannot find name 'Bar' | error[TK2304]: Cannot find name 'Baz'

interface I { m: Qux; }               // error[TK2304]: Cannot find name 'Qux'

type AliasOf = Zap;                   // error[TK2304]: Cannot find name 'Zap'

type UnionOf = number | Nope;         // error[TK2304]: Cannot find name 'Nope'

type ArrayOf = Missing[];             // error[TK2304]: Cannot find name 'Missing'

type TupleOf = [number, Gone];        // error[TK2304]: Cannot find name 'Gone'
