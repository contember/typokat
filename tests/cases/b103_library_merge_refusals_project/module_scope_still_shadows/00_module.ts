// Backlog 103 control across files. An external module's `interface Array<T>` and local `Date`
// shadow inside this file only — they must not publish, must not refuse, and must not be
// visible to the sibling script file.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2322 x2.
export {};

interface Array<T> {
  b103CrossElement: T;
}
class Date {
  b103CrossStamp(): boolean {
    return true;
  }
}

declare const moduleArray: Array<number>;
const moduleElement: number = moduleArray.b103CrossElement;
const wrongModuleElement: string = moduleArray.b103CrossElement; // error[TK2322]: Type 'number' is not assignable to type 'string'

const moduleStamp: boolean = new Date().b103CrossStamp();
const wrongModuleStamp: string = new Date().b103CrossStamp(); // error[TK2322]: Type 'boolean' is not assignable to type 'string'
