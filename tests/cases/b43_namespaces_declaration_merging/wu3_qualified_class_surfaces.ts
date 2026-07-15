// tsc 6.0.3 --strict --noEmit: TS2344 x3, TS2322 x6, and TS2345 x3 below.
declare namespace Wu3QualifiedClassSurfaces { // incomplete[decl/module-declaration/self]
  interface Contract { kind: "contract" }
  type Alias<T> = { value: T };
  class Box<T> { value: T }
}

declare class InterfaceConstrained<T extends Wu3QualifiedClassSurfaces.Contract> {}
declare class AliasConstrained<T extends Wu3QualifiedClassSurfaces.Alias<string>> {}
declare class ClassConstrained<T extends Wu3QualifiedClassSurfaces.Box<number>> {}

declare const badInterfaceConstraint: InterfaceConstrained<{ kind: "wrong" }>; // error[TK2344]: Type '{ kind: "wrong"; }' does not satisfy the constraint 'Contract'
declare const badAliasConstraint: AliasConstrained<{ value: number }>; // error[TK2344]: Type '{ value: number; }' does not satisfy the constraint '{ value: string; }'
declare const badClassConstraint: ClassConstrained<{ value: string }>; // error[TK2344]: Type '{ value: string; }' does not satisfy the constraint 'Box<number>'

declare class QualifiedSurfaceConsumer {
  interfaceField: Wu3QualifiedClassSurfaces.Contract;
  aliasField: Wu3QualifiedClassSurfaces.Alias<string>;
  classField: Wu3QualifiedClassSurfaces.Box<number>;

  interfaceMethod(value: Wu3QualifiedClassSurfaces.Contract): Wu3QualifiedClassSurfaces.Contract;
  aliasMethod(value: Wu3QualifiedClassSurfaces.Alias<string>): Wu3QualifiedClassSurfaces.Alias<string>;
  classMethod(value: Wu3QualifiedClassSurfaces.Box<number>): Wu3QualifiedClassSurfaces.Box<number>;
}

declare const qualifiedSurfaceConsumer: QualifiedSurfaceConsumer;

const interfaceFieldGood: "contract" = qualifiedSurfaceConsumer.interfaceField.kind;
const interfaceFieldBad: "wrong" = qualifiedSurfaceConsumer.interfaceField.kind; // error[TK2322]: Type '"contract"' is not assignable to type '"wrong"'
const aliasFieldGood: string = qualifiedSurfaceConsumer.aliasField.value;
const aliasFieldBad: number = qualifiedSurfaceConsumer.aliasField.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const classFieldGood: number = qualifiedSurfaceConsumer.classField.value;
const classFieldBad: string = qualifiedSurfaceConsumer.classField.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

const interfaceReturnGood: "contract" = qualifiedSurfaceConsumer.interfaceMethod({ kind: "contract" }).kind;
const interfaceReturnBad: "wrong" = qualifiedSurfaceConsumer.interfaceMethod({ kind: "contract" }).kind; // error[TK2322]: Type '"contract"' is not assignable to type '"wrong"'
const aliasReturnGood: string = qualifiedSurfaceConsumer.aliasMethod({ value: "ok" }).value;
const aliasReturnBad: number = qualifiedSurfaceConsumer.aliasMethod({ value: "ok" }).value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const classReturnGood: number = qualifiedSurfaceConsumer.classMethod({ value: 1 }).value;
const classReturnBad: string = qualifiedSurfaceConsumer.classMethod({ value: 1 }).value; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const wrongInterfaceArgument: { kind: "wrong" };
declare const wrongAliasArgument: { value: number };
declare const wrongClassArgument: { value: string };
qualifiedSurfaceConsumer.interfaceMethod(wrongInterfaceArgument); // error[TK2345]: Argument of type '{ kind: "wrong"; }' is not assignable to parameter of type 'Contract'
qualifiedSurfaceConsumer.aliasMethod(wrongAliasArgument); // error[TK2345]: Argument of type '{ value: number; }' is not assignable to parameter of type '{ value: string; }'
qualifiedSurfaceConsumer.classMethod(wrongClassArgument); // error[TK2345]: Argument of type '{ value: string; }' is not assignable to parameter of type 'Box<number>'
