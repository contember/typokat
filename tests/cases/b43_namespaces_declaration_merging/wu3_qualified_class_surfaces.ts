// tsc 6.0.3 --strict --noEmit: TS2315, TS2314, TS2707, TS2344 x5, TS2322 x12, and TS2345 x3 below.
declare namespace Wu3QualifiedClassSurfaces { // incomplete[decl/module-declaration/self]
  interface Contract { kind: "contract" }
  type Alias<T> = { value: T };
  type Pair<T, U> = { left: T; right: U };
  type Dependent<T, U = T> = { first: T; second: U };
  type ConstrainedAlias<T extends number> = { value: T };
  class Box<T> { value: T }
  interface Ranged<T, U = string> { first: T; second: U }
  class ConstrainedBox<T extends number> { value: T }
}

declare class InterfaceConstrained<T extends Wu3QualifiedClassSurfaces.Contract> {}
declare class AliasConstrained<T extends Wu3QualifiedClassSurfaces.Alias<string>> {}
declare class ClassConstrained<T extends Wu3QualifiedClassSurfaces.Box<number>> {}

declare const badInterfaceConstraint: InterfaceConstrained<{ kind: "wrong" }>; // error[TK2344]: Type '{ kind: "wrong"; }' does not satisfy the constraint 'Contract'
declare const badAliasConstraint: AliasConstrained<{ value: number }>; // error[TK2344]: Type '{ value: number; }' does not satisfy the constraint '{ value: string; }'
declare const badClassConstraint: ClassConstrained<{ value: string }>; // error[TK2344]: Type '{ value: string; }' does not satisfy the constraint 'Box<number>'

declare class QualifiedNonGenericApplication {
  field: Wu3QualifiedClassSurfaces.Contract<string>; // error[TK2315]: Type 'Contract' is not generic
}
declare class QualifiedRequiredArityApplication {
  field: Wu3QualifiedClassSurfaces.Pair<string>; // error[TK2314]: Generic type 'Pair' requires 2 type argument(s)
}
declare class QualifiedRangeArityApplication {
  field: Wu3QualifiedClassSurfaces.Ranged<string, number, boolean>; // error[TK2707]: Generic type 'Ranged<T, U>' requires between 1 and 2 type arguments
}
declare class QualifiedAliasConstraintApplication {
  field: Wu3QualifiedClassSurfaces.ConstrainedAlias<string>; // error[TK2344]: Type 'string' does not satisfy the constraint 'number'
}
declare class QualifiedClassConstraintApplication {
  field: Wu3QualifiedClassSurfaces.ConstrainedBox<string>; // error[TK2344]: Type 'string' does not satisfy the constraint 'number'
}

declare const qualifiedAliasConstraintApplication: QualifiedAliasConstraintApplication;
const qualifiedAliasConstraintRecoveryGood: string = qualifiedAliasConstraintApplication.field.value;
const qualifiedAliasConstraintRecoveryBad: number = qualifiedAliasConstraintApplication.field.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
declare const qualifiedClassConstraintApplication: QualifiedClassConstraintApplication;
const qualifiedClassConstraintRecoveryGood: string = qualifiedClassConstraintApplication.field.value;
const qualifiedClassConstraintRecoveryBad: number = qualifiedClassConstraintApplication.field.value; // error[TK2322]: Type 'string' is not assignable to type 'number'

declare class QualifiedSurfaceConsumer {
  interfaceField: Wu3QualifiedClassSurfaces.Contract;
  aliasField: Wu3QualifiedClassSurfaces.Alias<string>;
  classField: Wu3QualifiedClassSurfaces.Box<number>;
  rangedDefaultField: Wu3QualifiedClassSurfaces.Ranged<number>;
  dependentDefaultField: Wu3QualifiedClassSurfaces.Dependent<string>;

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
const rangedDefaultFirstGood: number = qualifiedSurfaceConsumer.rangedDefaultField.first;
const rangedDefaultSecondGood: string = qualifiedSurfaceConsumer.rangedDefaultField.second;
const rangedDefaultFirstBad: string = qualifiedSurfaceConsumer.rangedDefaultField.first; // error[TK2322]: Type 'number' is not assignable to type 'string'
const rangedDefaultSecondBad: number = qualifiedSurfaceConsumer.rangedDefaultField.second; // error[TK2322]: Type 'string' is not assignable to type 'number'
const dependentDefaultFirstGood: string = qualifiedSurfaceConsumer.dependentDefaultField.first;
const dependentDefaultSecondGood: string = qualifiedSurfaceConsumer.dependentDefaultField.second;
const dependentDefaultFirstBad: number = qualifiedSurfaceConsumer.dependentDefaultField.first; // error[TK2322]: Type 'string' is not assignable to type 'number'
const dependentDefaultSecondBad: number = qualifiedSurfaceConsumer.dependentDefaultField.second; // error[TK2322]: Type 'string' is not assignable to type 'number'

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
