// WU0 addendum — one side may introduce a constraint/default; conflicting supplied headers reject.
class GenericClassFirst<T extends { id: number } = { id: number }> {
  static existing: number = 1;
  constructor(readonly value: T) {}
}
interface GenericClassFirst<T extends { id: number } = { id: number }> {
  payload: T;
  recursive: GenericClassFirst<T>;
  generic<U>(value: U): U;
}
namespace GenericClassFirst {
  export const added: string = "generic-class-first";
  export interface Options<T> { value: T }
}

const genericClassFirst = new GenericClassFirst({ id: 1 });
const genericClassFirstPayloadWrong: string = genericClassFirst.payload.id; // error[TK2322]: Type 'number' is not assignable to type 'string'
const genericClassFirstRecursiveWrong: string = genericClassFirst.recursive.payload.id; // error[TK2322]: Type 'number' is not assignable to type 'string'
const genericClassFirstMethodWrong: number = genericClassFirst.generic("bad"); // error[TK2322]: Type 'string' is not assignable to type 'number'
const genericClassFirstExistingWrong: string = GenericClassFirst.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const genericClassFirstAddedWrong: number = GenericClassFirst.added; // error[TK2322]: Type 'string' is not assignable to type 'number'
const genericClassFirstOptions: GenericClassFirst.Options<string> = { value: "ok" };

interface GenericInterfaceFirst<T extends { id: number } = { id: number }> {
  payload: T;
  recursive: GenericInterfaceFirst<T>;
}
class GenericInterfaceFirst<T extends { id: number } = { id: number }> {
  static existing: number = 2;
  constructor(readonly value: T) {}
}
namespace GenericInterfaceFirst {
  export const added: string = "generic-interface-first";
}
const genericInterfaceFirst = new GenericInterfaceFirst({ id: 2 });
const genericInterfaceFirstPayloadWrong: string = genericInterfaceFirst.payload.id; // error[TK2322]: Type 'number' is not assignable to type 'string'
const genericInterfaceFirstRecursiveWrong: string = genericInterfaceFirst.recursive.payload.id; // error[TK2322]: Type 'number' is not assignable to type 'string'
const genericInterfaceFirstExistingWrong: string = GenericInterfaceFirst.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const genericInterfaceFirstAddedWrong: number = GenericInterfaceFirst.added; // error[TK2322]: Type 'string' is not assignable to type 'number'

class RenamedClassHeader<T> { // error[TK2428]: All declarations of 'RenamedClassHeader' must have identical type parameters
  constructor(readonly classValue: T) {}
}
interface RenamedClassHeader<U> { // error[TK2428]: All declarations of 'RenamedClassHeader' must have identical type parameters
  interfaceValue: U;
}
declare const renamedHeaderRecovery: RenamedClassHeader<string, number>;
const renamedClassValueWrong: boolean = renamedHeaderRecovery.classValue; // error[TK2322]: Type 'string' is not assignable to type 'boolean'
const renamedInterfaceValueWrong: boolean = renamedHeaderRecovery.interfaceValue; // error[TK2322]: Type 'number' is not assignable to type 'boolean'

class ConstraintClassHeader<T extends string> { // error[TK2428]: All declarations of 'ConstraintClassHeader' must have identical type parameters
  constructor(readonly classValue: T) {}
}
interface ConstraintClassHeader<T extends number> { // error[TK2428]: All declarations of 'ConstraintClassHeader' must have identical type parameters
  interfaceValue: T;
}
declare const constraintHeaderRecovery: ConstraintClassHeader<string>;
const constraintClassValueWrong: boolean = constraintHeaderRecovery.classValue; // error[TK2322]: Type 'string' is not assignable to type 'boolean'
const constraintInterfaceValueWrong: boolean = constraintHeaderRecovery.interfaceValue; // error[TK2322]: Type 'string' is not assignable to type 'boolean'
let constraintHeaderRejected: ConstraintClassHeader<number>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'

interface DefaultInterfaceHeader<T extends string = "interface"> { // error[TK2428]: All declarations of 'DefaultInterfaceHeader' must have identical type parameters
  interfaceValue: T;
}
class DefaultInterfaceHeader<T extends string = "class"> { // error[TK2428]: All declarations of 'DefaultInterfaceHeader' must have identical type parameters
  constructor(readonly classValue: T) {}
}
declare const defaultHeaderRecovery: DefaultInterfaceHeader;
const defaultClassValueWrong: "other" = defaultHeaderRecovery.classValue; // error[TK2322]: Type '"interface"' is not assignable to type '"other"'
const defaultInterfaceValueWrong: "other" = defaultHeaderRecovery.interfaceValue; // error[TK2322]: Type '"interface"' is not assignable to type '"other"'

class ClassOmitConstraint<T> {
  constructor(readonly value: T) {}
}
interface ClassOmitConstraint<T extends string> { added: T }
const classOmitConstraint = new ClassOmitConstraint<"ok">("ok");
const classOmitConstraintWrong: number = classOmitConstraint.added; // error[TK2322]: Type 'string' is not assignable to type 'number'
let classOmitConstraintBad: ClassOmitConstraint<number>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'

interface InterfaceOmitConstraint<T> { added: T }
class InterfaceOmitConstraint<T extends string> {
  constructor(readonly value: T) {}
}
const interfaceOmitConstraint = new InterfaceOmitConstraint<"ok">("ok");
const interfaceOmitConstraintWrong: number = interfaceOmitConstraint.added; // error[TK2322]: Type 'string' is not assignable to type 'number'
let interfaceOmitConstraintBad: InterfaceOmitConstraint<number>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'

class ClassOmitDefault<T> {
  constructor(readonly value?: T) {}
}
interface ClassOmitDefault<T = "class-default"> { added: T }
declare const classOmitDefault: ClassOmitDefault;
const classOmitDefaultLiteral: "class-default" = classOmitDefault.added;
const classOmitDefaultWrong: "other" = classOmitDefault.added; // error[TK2322]: Type '"class-default"' is not assignable to type '"other"'

interface InterfaceOmitDefault<T> { added: T }
class InterfaceOmitDefault<T = "interface-default"> {
  constructor(readonly value?: T) {}
}
declare const interfaceOmitDefault: InterfaceOmitDefault;
const interfaceOmitDefaultLiteral: "interface-default" = interfaceOmitDefault.added;
const interfaceOmitDefaultWrong: "other" = interfaceOmitDefault.added; // error[TK2322]: Type '"interface-default"' is not assignable to type '"other"'

class ArityClassHeader<T> { // error[TK2428]: All declarations of 'ArityClassHeader' must have identical type parameters
  constructor(readonly value: T) {}
}
interface ArityClassHeader<T, U> { // error[TK2428]: All declarations of 'ArityClassHeader' must have identical type parameters
  extra: U;
}
declare const arityRecovery: ArityClassHeader<string, number>;
const arityValueWrong: boolean = arityRecovery.value; // error[TK2322]: Type 'string' is not assignable to type 'boolean'
const arityExtraWrong: boolean = arityRecovery.extra; // error[TK2322]: Type 'number' is not assignable to type 'boolean'
