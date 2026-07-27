// A script file in the same project. Nothing the module above declared may be visible here: the
// library `Array` and `Date` surfaces must be intact, and the module-local class must be unknown.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2339 x2, TS2304 x2 and
// TS2322 x2 here.
declare const scriptArray: Array<number>;
scriptArray.b102ModuleOnlyElement; // error[TK2339]
const scriptLength: number = scriptArray.length;
const wrongScriptLength: string = scriptArray.length; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const scriptDate: Date;
scriptDate.b102ModuleOnlyStamp; // error[TK2339]
const scriptTime: number = scriptDate.getTime();
const wrongScriptTime: string = scriptDate.getTime(); // error[TK2322]: Type 'number' is not assignable to type 'string'

const moduleOnlyClassValue = new B102ModuleOnlyClass(); // error[TK2304]: Cannot find name 'B102ModuleOnlyClass'
declare const moduleOnlyClassAnnotation: B102ModuleOnlyClass; // error[TK2304]
