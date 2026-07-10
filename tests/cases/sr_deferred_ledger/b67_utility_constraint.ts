// Deferred ledger / backlog 67 — prelude utility aliases omit their lib-style
// type-parameter constraints, so a violating type argument evaluates instead of
// erroring — a dropped tsc TS2344. `ReturnType<number>` evaluates to `never` and
// is silent; tsc rejects it. This corpus stays DISABLED beyond this sprint (until
// backlog 67 enforces the alias constraint without a permissive `any[]`
// shortcut). Cross-checked vs tsc 6.0.3 --strict. Asserted code-only (the
// constraint is a function type).

// witness (dropped error): `number` does not satisfy the `ReturnType` constraint
// `(...args: any) => any` — tsc: TS2344. typokat evaluates to `never`, silent.
type R = ReturnType<number>; // error[TK2344]

// --- control: a valid `ReturnType` argument stays clean and evaluates to
// `string`. ---
type Ok = ReturnType<() => string>;
const useOk: Ok = "s";
