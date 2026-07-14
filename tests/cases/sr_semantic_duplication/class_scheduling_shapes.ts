// Semantic-duplication architecture gate — finite declaration-graph scheduling shapes.
// Cross-checked with tsc 6.0.3 --strict; only the deliberate primitive mismatches report.

class AliasBoundaryLeft<T> {
  bridge!: AliasBoundaryBridge<T>;
}

type AliasBoundaryBridge<T> = AliasBoundaryRight<T>;

class AliasBoundaryRight<T> {
  back!: AliasBoundaryLeft<T>;
  value!: T;
}

declare const aliasBoundary: AliasBoundaryLeft<string>;
const aliasBoundaryGood: string = aliasBoundary.bridge.back.bridge.value;
const aliasBoundaryBad: number = aliasBoundary.bridge.back.bridge.value; // error[TK2322]: Type 'string' is not assignable to type 'number'

class InterfaceBoundaryLeft<T> {
  bridge!: InterfaceBoundaryBridge<T>;
}

interface InterfaceBoundaryBridge<T> {
  target: InterfaceBoundaryRight<T>;
}

class InterfaceBoundaryRight<T> {
  back!: InterfaceBoundaryLeft<T>;
  value!: T;
}

declare const interfaceBoundary: InterfaceBoundaryLeft<string>;
const interfaceBoundaryGood: string = interfaceBoundary.bridge.target.back.bridge.target.value;
const interfaceBoundaryBad: number = interfaceBoundary.bridge.target.back.bridge.target.value; // error[TK2322]: Type 'string' is not assignable to type 'number'

class InterfaceClassBase<T> {
  value!: T;
}

interface InterfaceExtendsClass<T> extends InterfaceClassBase<T> {
  next: InterfaceExtendsClass<T>;
}

declare const interfaceExtendsClass: InterfaceExtendsClass<string>;
const interfaceExtendsClassGood: string = interfaceExtendsClass.next.value;
const interfaceExtendsClassBad: number = interfaceExtendsClass.next.value; // error[TK2322]: Type 'string' is not assignable to type 'number'

type ObjectBoundary<T> = { owner: ObjectBoundaryClass<T> };
class ObjectBoundaryClass<T> {
  boundary!: ObjectBoundary<T>;
  value!: T;
}

declare const objectBoundary: ObjectBoundaryClass<string>;
const objectBoundaryGood: string = objectBoundary.boundary.owner.value;
const objectBoundaryBad: number = objectBoundary.boundary.owner.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
