// WU3 / finding 9 — `replace_mapped_value` (eval/mapped.rs:453) recurses with no
// cycle guard. A mapped type whose VALUE template is a recursive object type
// recurses unboundedly and STACK-OVERFLOWS the process (SIGABRT) at HEAD —
// verified: `cargo run -- check` on this file aborts with
// "thread 'main' has overflowed its stack". WU3 must bound the recursion (its
// own in-progress/memo context) and produce the pinned tsc verdict instead of a
// native crash. Cross-checked vs tsc 6.0.3 --strict: TS2322 (the mapped result
// is an object type, not `number`); asserted code-only (recursive object source).
//
// NOTE: while this dir is DISABLED, `cargo test` never loads this file. When WU3
// enables the dir, the recursion guard must already be in place or `cargo test`
// aborts.

type Rec = { self: Rec };
type Wrap<T> = { [K in keyof T]: Rec };
type Applied = Wrap<{ a: 1 }>;

declare const x: Applied;
const bad: number = x; // error[TK2322]

// --- control: a non-recursive object value template resolves and checks
// normally (no unbounded recursion). ---
type WrapPlain<T> = { [K in keyof T]: { n: number } };
type AppliedPlain = WrapPlain<{ a: 1 }>;
declare const y: AppliedPlain;
const bad2: number = y; // error[TK2322]
