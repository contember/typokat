// backlog 92 - one error nested inside contextually typed arrow arguments is
// reported once, not 2^d times.
//
// `run`'s parameter type STRUCTURALLY contains the type variable, so the argument
// is walked three times per level (raw, candidate inference, committed check) and
// two of those walks retain effects. tsc 6.0.3 --strict reports exactly one TS2304
// on each line below, at every depth; typokat reports 2^depth copies that are
// byte-identical in code, message, and span.
//
// One marker per line asserts exactly one diagnostic: the harness compares the
// per-line MULTISET of codes, so a duplicate fails as a code mismatch.

declare function run<T>(step: (value: number) => T): T;

const arrows1 = run(v0 => undeclaredThing); // error[TK2304]: Cannot find name 'undeclaredThing'
const arrows2 = run(v0 => run(v1 => undeclaredThing)); // error[TK2304]: Cannot find name 'undeclaredThing'
const arrows3 = run(v0 => run(v1 => run(v2 => undeclaredThing))); // error[TK2304]: Cannot find name 'undeclaredThing'
const arrows4 = run(v0 => run(v1 => run(v2 => run(v3 => undeclaredThing)))); // error[TK2304]: Cannot find name 'undeclaredThing'
const arrows5 = run(v0 => run(v1 => run(v2 => run(v3 => run(v4 => undeclaredThing))))); // error[TK2304]: Cannot find name 'undeclaredThing'
const arrows6 = run(v0 => run(v1 => run(v2 => run(v3 => run(v4 => run(v5 => undeclaredThing)))))); // error[TK2304]: Cannot find name 'undeclaredThing'
const arrows7 = run(v0 => run(v1 => run(v2 => run(v3 => run(v4 => run(v5 => run(v6 => undeclaredThing))))))); // error[TK2304]: Cannot find name 'undeclaredThing'
const arrows8 = run(v0 => run(v1 => run(v2 => run(v3 => run(v4 => run(v5 => run(v6 => run(v7 => undeclaredThing)))))))); // error[TK2304]: Cannot find name 'undeclaredThing'
