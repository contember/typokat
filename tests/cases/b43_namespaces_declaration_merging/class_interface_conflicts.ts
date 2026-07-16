// WU0 addendum — property conflicts diagnose in both orders and retain a non-permissive class type.
class ClassFirstPropertyConflict {
  value = 1;
}
interface ClassFirstPropertyConflict {
  value: string; // error[TK2717]: Subsequent property declarations must have the same type
}
declare const classFirstConflict: ClassFirstPropertyConflict;
const classFirstConflictStillNumber: number = classFirstConflict.value;
const classFirstConflictWrong: boolean = classFirstConflict.value; // error[TK2322]

interface InterfaceFirstPropertyConflict {
  value: string;
}
class InterfaceFirstPropertyConflict {
  value = 1; // error[TK2717]: Subsequent property declarations must have the same type
}
declare const interfaceFirstConflict: InterfaceFirstPropertyConflict;
const interfaceFirstConflictStillString: string = interfaceFirstConflict.value;
const interfaceFirstConflictWrong: boolean = interfaceFirstConflict.value; // error[TK2322]

class PropertyThenMethod {
  entry = 1; // error[TK2300]: Duplicate identifier 'entry'
}
interface PropertyThenMethod {
  entry(): string; // error[TK2300]: Duplicate identifier 'entry'
}
declare const propertyThenMethod: PropertyThenMethod;
propertyThenMethod.entry(); // error[TK2349]: This expression is not callable
const propertyThenMethodWrong: boolean = propertyThenMethod.entry; // error[TK2322]: Type 'number' is not assignable to type 'boolean'

class MethodThenProperty {
  entry(): number { return 1; }
}
interface MethodThenProperty {
  entry: string; // error[TK2717]: Subsequent property declarations must have the same type
}
declare const methodThenProperty: MethodThenProperty;
const methodThenPropertyCall: number = methodThenProperty.entry();
const methodThenPropertyWrong: boolean = methodThenProperty.entry; // error[TK2322]

interface OptionalClassConflict {
  value?: number; // error[TK2687]: All declarations of 'value' must have identical modifiers
}
class OptionalClassConflict {
  value = 1; // error[TK2687]: All declarations of 'value' must have identical modifiers | error[TK2717]: Subsequent property declarations must have the same type
}

interface ReadonlyClassConflict {
  readonly value: number; // error[TK2687]: All declarations of 'value' must have identical modifiers
}
class ReadonlyClassConflict {
  value = 1; // error[TK2687]: All declarations of 'value' must have identical modifiers
}

interface HeritageCollisionBase { value: string }
class HeritageCollision {
  value = 1;
}
interface HeritageCollision extends HeritageCollisionBase {} // error[TK2430]: incorrectly extends interface 'HeritageCollisionBase'
declare const heritageCollision: HeritageCollision;
const heritageCollisionWrong: boolean = heritageCollision.value; // error[TK2322]: Type 'number' is not assignable to type 'boolean'

class CompatibleMethodOverloads {
  method(value: number): number;
  method(value: number | string): number | string { return value; }
}
interface CompatibleMethodOverloads {
  method(value: string): string;
}
declare const compatibleMethodOverloads: CompatibleMethodOverloads;
const compatibleNumberCall: number = compatibleMethodOverloads.method(1);
const compatibleStringCall: string = compatibleMethodOverloads.method("one");
compatibleMethodOverloads.method(true); // error[TK2769]

class InstanceSignatureMerge { instance = 1; }
interface InstanceSignatureMerge {
  (value: number): string;
  new (value: number): { result: string };
}
declare const instanceSignatureMerge: InstanceSignatureMerge;
const instanceCallWrong: number = instanceSignatureMerge(1); // error[TK2322]: Type 'string' is not assignable to type 'number'
const instanceConstructWrong: number = new instanceSignatureMerge(1).result; // error[TK2322]: Type 'string' is not assignable to type 'number'

interface InterfaceMethodThenClassProperty {
  entry(): string;
}
class InterfaceMethodThenClassProperty {
  entry = 1; // error[TK2717]: Subsequent property declarations must have the same type
}
declare const interfaceMethodThenClassProperty: InterfaceMethodThenClassProperty;
const interfaceMethodCall: string = interfaceMethodThenClassProperty.entry();
const interfaceMethodThenPropertyWrong: boolean = interfaceMethodThenClassProperty.entry; // error[TK2322]

interface InterfaceFirstCompatibleOverloads {
  method(value: string): string;
}
class InterfaceFirstCompatibleOverloads {
  method(value: number): number;
  method(value: number | string): number | string { return value; }
}
declare const interfaceFirstCompatibleOverloads: InterfaceFirstCompatibleOverloads;
const interfaceFirstStringCall: string = interfaceFirstCompatibleOverloads.method("one");
const interfaceFirstNumberCall: number = interfaceFirstCompatibleOverloads.method(1);
interfaceFirstCompatibleOverloads.method(true); // error[TK2769]

interface InterfaceFirstInstanceSignatures {
  (value: number): string;
  new (value: number): { result: string };
}
class InterfaceFirstInstanceSignatures { instance = 1; }
declare const interfaceFirstInstanceSignatures: InterfaceFirstInstanceSignatures;
const interfaceFirstCallWrong: number = interfaceFirstInstanceSignatures(1); // error[TK2322]: Type 'string' is not assignable to type 'number'
const interfaceFirstConstructWrong: number = new interfaceFirstInstanceSignatures(1).result; // error[TK2322]: Type 'string' is not assignable to type 'number'

interface InterfaceFirstHeritageCollision extends InterfaceFirstHeritageBase {} // error[TK2430]: incorrectly extends interface 'InterfaceFirstHeritageBase'
interface InterfaceFirstHeritageBase { value: string }
class InterfaceFirstHeritageCollision { value = 1; }
declare const interfaceFirstHeritageCollision: InterfaceFirstHeritageCollision;
const interfaceFirstHeritageWrong: boolean = interfaceFirstHeritageCollision.value; // error[TK2322]: Type 'number' is not assignable to type 'boolean'

class PrivatePropertyAccessibilityConflict {
  private value: number = 1; // error[TK2687]: All declarations of 'value' must have identical modifiers
}
interface PrivatePropertyAccessibilityConflict {
  value: number; // error[TK2687]: All declarations of 'value' must have identical modifiers
}
declare const privatePropertyAccessibilityConflict: PrivatePropertyAccessibilityConflict;
privatePropertyAccessibilityConflict.value; // error[TK2341]: Property 'value' is private

class ProtectedPropertyAccessibilityConflict {
  protected value: number = 1; // error[TK2687]: All declarations of 'value' must have identical modifiers
}
interface ProtectedPropertyAccessibilityConflict {
  value: number; // error[TK2687]: All declarations of 'value' must have identical modifiers
}
declare const protectedPropertyAccessibilityConflict: ProtectedPropertyAccessibilityConflict;
protectedPropertyAccessibilityConflict.value; // error[TK2445]: Property 'value' is protected

class PrivateMethodAccessibilityConflict {
  private method(value: number): number { return value; }
}
interface PrivateMethodAccessibilityConflict {
  method(value: number): number; // error[TK2385]: Overload signatures must all be public, private or protected
}
declare const privateMethodAccessibilityConflict: PrivateMethodAccessibilityConflict;
privateMethodAccessibilityConflict.method(1); // error[TK2341]: Property 'method' is private

class ProtectedMethodAccessibilityConflict {
  protected method(value: number): number { return value; }
}
interface ProtectedMethodAccessibilityConflict {
  method(value: number): number; // error[TK2385]: Overload signatures must all be public, private or protected
}
declare const protectedMethodAccessibilityConflict: ProtectedMethodAccessibilityConflict;
protectedMethodAccessibilityConflict.method(1); // error[TK2445]: Property 'method' is protected

class GetterPropertyTypeConflict {
  get value(): number { return 1; }
}
interface GetterPropertyTypeConflict {
  value: string; // error[TK2717]: Subsequent property declarations must have the same type
}
declare const getterPropertyTypeConflict: GetterPropertyTypeConflict;
const getterPropertyTypeConflictNumber: number = getterPropertyTypeConflict.value;
const getterPropertyTypeConflictWrong: string = getterPropertyTypeConflict.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

class ClassOwnedHeritageBase {
  inherited: number = 1;
}
class ClassOwnedHeritage {}
interface ClassOwnedHeritage extends ClassOwnedHeritageBase {}
const classOwnedHeritageNumber: number = new ClassOwnedHeritage().inherited;
const classOwnedHeritageWrong: string = new ClassOwnedHeritage().inherited; // error[TK2322]: Type 'number' is not assignable to type 'string'

type ClassFirstIntersectionHeritageShape = { value: string } & { other: boolean };
type ClassFirstIntersectionHeritageAlias = ClassFirstIntersectionHeritageShape;
class ClassFirstIntersectionHeritage {
  value: number = 1;
}
interface ClassFirstIntersectionHeritage extends ClassFirstIntersectionHeritageAlias {} // error[TK2430]
const classFirstIntersectionValue: number = new ClassFirstIntersectionHeritage().value;
const classFirstIntersectionOther: boolean = new ClassFirstIntersectionHeritage().other;
const classFirstIntersectionValueWrong: string = new ClassFirstIntersectionHeritage().value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const classFirstIntersectionOtherWrong: number = new ClassFirstIntersectionHeritage().other; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

type InterfaceFirstIntersectionHeritageShape = { value: string } & { other: boolean };
type InterfaceFirstIntersectionHeritageAlias = InterfaceFirstIntersectionHeritageShape;
interface InterfaceFirstIntersectionHeritage extends InterfaceFirstIntersectionHeritageAlias {} // error[TK2430]
class InterfaceFirstIntersectionHeritage {
  value: number = 1;
}
const interfaceFirstIntersectionValue: number = new InterfaceFirstIntersectionHeritage().value;
const interfaceFirstIntersectionOther: boolean = new InterfaceFirstIntersectionHeritage().other;
const interfaceFirstIntersectionValueWrong: string = new InterfaceFirstIntersectionHeritage().value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const interfaceFirstIntersectionOtherWrong: number = new InterfaceFirstIntersectionHeritage().other; // error[TK2322]: Type 'boolean' is not assignable to type 'number'
