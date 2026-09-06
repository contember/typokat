class OpaqueRoot<T extends number[]> {
  fn = ((value) => 1) satisfies (...args: T) => number; // error[TK1360] | incomplete[class/property-definition/initializer-inference]
}
class OpaqueDefault<T extends number[] = number[]> { // incomplete[annotation-lower/type-parameter-default/self]
  fn = ((value) => 1) satisfies (...args: T) => number; // error[TK1360] | incomplete[class/property-definition/initializer-inference]
}
class OpaqueNested<T extends number[]> {
  fn = ((value) => 1) satisfies (...args: [...T]) => number; // error[TK1360] | incomplete[class/property-definition/initializer-inference]
}
class OpaqueUnion<T extends number[]> {
  fn = ((value) => 1) satisfies (...args: T | number[]) => number; // error[TK1360] | incomplete[class/property-definition/initializer-inference]
}
class AnnotatedOpaque<T extends number[]> {
  fn = ((value: number) => 1) satisfies (...args: T) => number; // error[TK1360]
}
class SymbolicElement<T> {
  array = ((value) => value) satisfies (...args: T[]) => T;
  fixed = ((value) => value) satisfies (value: T) => T;
}
class UnusedOpaqueRest<T extends number[]> {
  fixedPrefix = ((value) => value) satisfies (value: number, ...args: T) => number; // error[TK1360]
}
