// Backlog 103 control. The guard must fire ONLY at the frozen global. Inside an external
// module the same spellings are module-local: they SHADOW the library type and must neither
// merge, nor refuse, nor record anything. Nothing here may be an incomplete outcome.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2322 x2 and TS2339 x2.
export {};

interface Array<T> {
  b103ModuleElement: T;
}
class Date {
  b103ModuleStamp(): boolean {
    return true;
  }
}

declare const localArray: Array<number>;
const localElement: number = localArray.b103ModuleElement;
const wrongLocalElement: string = localArray.b103ModuleElement; // error[TK2322]: Type 'number' is not assignable to type 'string'

const localDate = new Date();
const localStamp: boolean = localDate.b103ModuleStamp();
const wrongLocalStamp: string = localDate.b103ModuleStamp(); // error[TK2322]: Type 'boolean' is not assignable to type 'string'

// The native shapes keep the library surface: the module-local members are not on them.
[1, 2].b103ModuleElement; // error[TK2339]
"text".length.toFixed(0).b103ModuleStamp; // error[TK2339]
