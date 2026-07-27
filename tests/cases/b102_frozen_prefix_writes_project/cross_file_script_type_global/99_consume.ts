// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports exactly one TS2322 here.
// At the buggy HEAD typokat reports TK2304 on every line that names the global type instead.
declare const crossShape: B102CrossShape;

const crossShapeName: string = crossShape.name;
const wrongCrossShapeName: number = crossShape.name; // error[TK2322]: Type 'string' is not assignable to type 'number'
