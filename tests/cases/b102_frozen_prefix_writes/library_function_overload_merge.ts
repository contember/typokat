// Backlog 103 correctness tier. A user overload of the library's `parseInt` must be routed
// through the private collision epoch and appended to the existing overload set.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is CLEAN on the declaration and on both
// three-argument calls, and reports only the deliberate TS2322 on the last line.
declare function parseInt(text: string, radix: number, extra: number): number;

const parsedThree: number = parseInt("1", 2, 3);
const parsedTwo: number = parseInt("1", 2);
const parsedWrong: string = parseInt("1", 2, 3); // error[TK2322]: Type 'number' is not assignable to type 'string'
