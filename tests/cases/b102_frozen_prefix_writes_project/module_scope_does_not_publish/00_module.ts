// Backlog 102 control: an external module's top-level declarations must keep SHADOWING inside
// this file and must never reach the project-wide global scope the fix adds. `interface Array<T>`
// and a module-local `Date` are the two shapes that would corrupt the library surface if the fix
// published module-local names.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports exactly one TS2322 here.
export {};

interface Array<T> {
  b102ModuleOnlyElement: T;
}
interface Date {
  b102ModuleOnlyStamp: boolean;
}
class B102ModuleOnlyClass {
  value: number = 1;
}

declare const moduleArray: Array<number>;
const moduleElement: number = moduleArray.b102ModuleOnlyElement;
const wrongModuleElement: string = moduleArray.b102ModuleOnlyElement; // error[TK2322]: Type 'number' is not assignable to type 'string'
