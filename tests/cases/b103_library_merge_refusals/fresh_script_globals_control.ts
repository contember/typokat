// Backlog 103 control, the backlog-102 regression net. A script global whose name is FRESH
// still publishes into the delta-side global scope and refuses nothing — the guard must not
// widen into "every script global is refused".
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2322 once.
interface B103FreshShape {
  label: string;
}

declare var b103FreshValue: B103FreshShape;
declare function b103FreshCall(input: string): number;

const label: string = b103FreshValue.label;
const wrongLabel: number = b103FreshValue.label; // error[TK2322]: Type 'string' is not assignable to type 'number'
const called: number = b103FreshCall("x");
