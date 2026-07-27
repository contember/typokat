// Backlog 102, the fail-closed half — the shape where a dropped write becomes false positives at
// a distance. A user overload of the library's `parseInt` cannot be appended to the frozen
// function symbol, so the user's own three-argument call is rejected against the library's
// one-to-two-argument signature. Appending the overload is backlog 103's job; recording the
// refusal instead of dropping it is this item's.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is CLEAN on the declaration and on both
// three-argument calls, and reports only the deliberate TS2322 on the last line. Every TK2554
// below is therefore a false positive owned by backlog 103 and ledgered in
// docs/reference/divergences.md.
declare function parseInt(text: string, radix: number, extra: number): number; // incomplete[bind/frozen-library-global/merge-refused]

const parsedThree: number = parseInt("1", 2, 3); // error[TK2554]: Expected 1-2 arguments, but got 3
const parsedTwo: number = parseInt("1", 2);
const parsedWrong: string = parseInt("1", 2, 3); // error[TK2554]: Expected 1-2 arguments, but got 3 | error[TK2322]: Type 'number' is not assignable to type 'string'
