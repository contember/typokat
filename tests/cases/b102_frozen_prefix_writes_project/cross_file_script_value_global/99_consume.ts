// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports exactly one TS2322 here.
// At the buggy HEAD typokat reports TK2304 on both lines instead.
const crossValue: number = b102CrossValue;
const wrongCrossValue: string = b102CrossValue; // error[TK2322]: Type 'number' is not assignable to type 'string'
