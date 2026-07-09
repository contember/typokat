// M33 - overloaded object/interface call and construct signatures.
// Cross-checked against tsc 6.0.3 --strict.

interface InterfaceCallableOverload {
  (x: number): "n";
  (x: string): "s";
}

type ObjectCallableOverload = {
  (x: number): "n";
  (x: string): "s";
};

declare const interfaceOverload: InterfaceCallableOverload;
declare const objectOverload: ObjectCallableOverload;

const interfaceNumberResult: "n" = interfaceOverload(1);
const interfaceStringResult: "s" = interfaceOverload("x");
const interfaceBadResult: "s" = interfaceOverload(1); // error[TK2322]: Type '"n"' is not assignable to type '"s"'
interfaceOverload(true); // error[TK2769]: No overload matches this call

const objectNumberResult: "n" = objectOverload(1);
const objectStringResult: "s" = objectOverload("x");
const objectBadResult: "n" = objectOverload("x"); // error[TK2322]: Type '"s"' is not assignable to type '"n"'
objectOverload(true); // error[TK2769]: No overload matches this call

type ConstructOverload = {
  new (x: number): { kind: "n" };
  new (x: string): { kind: "s" };
};

declare const Constructable: ConstructOverload;

const constructedNumber: { kind: "n" } = new Constructable(1);
const constructedString: { kind: "s" } = new Constructable("x");
const constructedBad: { kind: "s" } = new Constructable(1); // error[TK2322]
new Constructable(true); // error[TK2769]: No overload matches this call
