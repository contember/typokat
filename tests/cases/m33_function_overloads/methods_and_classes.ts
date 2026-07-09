// M33 - overloaded methods and class constructor overloads.
// Cross-checked against tsc 6.0.3 --strict.

interface InterfaceMethodOverload {
  method(x: number): "n";
  method(x: string): "s";
}

declare const interfaceMethod: InterfaceMethodOverload;

const interfaceMethodNumber: "n" = interfaceMethod.method(1);
const interfaceMethodString: "s" = interfaceMethod.method("x");
const interfaceMethodBad: "s" = interfaceMethod.method(1); // error[TK2322]: Type '"n"' is not assignable to type '"s"'
interfaceMethod.method(true); // error[TK2769]: No overload matches this call

class ClassMethodOverload {
  method(x: number): "n";
  method(x: string): "s";
  method(x: number | string): "n" | "s" { return "n"; }
}

const classMethod = new ClassMethodOverload();
const classMethodNumber: "n" = classMethod.method(1);
const classMethodString: "s" = classMethod.method("x");
const classMethodBad: "n" = classMethod.method("x"); // error[TK2322]: Type '"s"' is not assignable to type '"n"'
classMethod.method(true); // error[TK2769]: No overload matches this call

class ConstructorOverload {
  value: number | string;
  constructor(x: number);
  constructor(x: string);
  constructor(x: number | string) {
    this.value = x;
  }
}

new ConstructorOverload(1);
new ConstructorOverload("x");
new ConstructorOverload(true); // error[TK2769]: No overload matches this call

class BadCtor {
  constructor(x: string); // error[TK2394]: not compatible with its implementation signature
  constructor(x: boolean) {}
}

class BadMethod {
  method(x: string): string; // error[TK2394]: not compatible with its implementation signature
  method(x: boolean): boolean { return x; }
}

class GenericClassMethodOverloadsDeferred {
  map<T>(x: T): T;
  map<T>(x: T[]): T[];
  map<T>(x: T | T[]): T | T[] { return x; }
}

const missingGenericClassMethod: GenericClassMethodOverloadsDeferred = {}; // error[TK2741]
