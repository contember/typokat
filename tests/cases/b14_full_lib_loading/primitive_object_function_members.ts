// tsc 6.0.3 --strict --target es2025: TS2322 x5 and TS2339 below.

const upper: string = "typokat".toUpperCase();
const wrongUpper: number = "typokat".toUpperCase(); // error[TK2322]: Type 'string' is not assignable to type 'number'

const fixed: string = (12.5).toFixed(1);
const wrongFixed: boolean = (12.5).toFixed(1); // error[TK2322]: Type 'string' is not assignable to type 'boolean'

const booleanValue: boolean = true.valueOf();
const wrongBooleanValue: string = true.valueOf(); // error[TK2322]: Type 'boolean' is not assignable to type 'string'
declare const objectValue: Object;
const objectText: string = objectValue.toString();
const wrongObjectText: number = objectValue.toString(); // error[TK2322]: Type 'string' is not assignable to type 'number'

function b14Increment(value: number): number {
  return value + 1;
}

const called: number = b14Increment.call(undefined, 1);
const wrongCalled: string = b14Increment.call(undefined, 1); // error[TK2322]: Type 'number' is not assignable to type 'string'
b14Increment.notAFunctionMember; // error[TK2339]
