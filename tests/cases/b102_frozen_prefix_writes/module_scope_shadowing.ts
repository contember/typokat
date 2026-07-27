// Backlog 102 control. An external module's top-level declarations are module-local: they
// SHADOW the library spelling inside this file and must never be published into any global
// scope. The delta-side global scope the cross-file fix adds must not capture them.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2339 x2 and TS2322 x2.
export {};

interface Array<T> {
  b102ModuleLocalElement: T;
}
interface Date {
  b102ModuleLocalStamp: boolean;
}

declare const shadowedArray: Array<number>;
const shadowedElement: number = shadowedArray.b102ModuleLocalElement;
const wrongShadowedElement: string = shadowedArray.b102ModuleLocalElement; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const shadowedDate: Date;
const shadowedStamp: boolean = shadowedDate.b102ModuleLocalStamp;
const wrongShadowedStamp: string = shadowedDate.b102ModuleLocalStamp; // error[TK2322]: Type 'boolean' is not assignable to type 'string'

// The native shapes keep the library surface: the module-local members are not on them.
[1, 2].b102ModuleLocalElement; // error[TK2339]
new Date().b102ModuleLocalStamp; // error[TK2339]
