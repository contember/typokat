// tsc 6.0.3 --strict --noEmit: TS2315, TS2707, and TS2344 below.
declare namespace Wu3QualifiedGenericLeaves {
  interface Plain { value: number }
  interface Box<T extends number = 1> { value: T }
  namespace Nested {
    interface Item<T> { value: T }
  }
}

type QualifiedNonGenericLeaf = Wu3QualifiedGenericLeaves.Plain<number>; // error[TK2315]: Type 'Plain' is not generic
type QualifiedWrongArityLeaf = Wu3QualifiedGenericLeaves.Box<1, 2>; // error[TK2707]: Generic type 'Box<T>' requires between 0 and 1 type arguments
type QualifiedConstraintLeaf = Wu3QualifiedGenericLeaves.Box<string>; // error[TK2344]: Type 'string' does not satisfy the constraint 'number'
type QualifiedNestedGenericLeaf = Wu3QualifiedGenericLeaves.Nested.Item<string>;

namespace Wu3QualifiedClassLeaves { // incomplete[decl/module-declaration/self]
  export class Generic<T> { value!: T }
  export class Constrained<T extends string> { value!: T }
  export class Dependent<T, U extends T> { value!: U }
  export class Plain { value!: number }
}

declare const qualifiedGenericClass: Wu3QualifiedClassLeaves.Generic<string>;
const qualifiedGenericClassValue: string = qualifiedGenericClass.value;
const qualifiedGenericClassWrong: number = qualifiedGenericClass.value; // error[TK2322]: Type 'string' is not assignable to type 'number'

declare const qualifiedConstrainedClass: Wu3QualifiedClassLeaves.Constrained<number>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
const qualifiedConstrainedClassValue: number = qualifiedConstrainedClass.value;
const qualifiedConstrainedClassWrong: string = qualifiedConstrainedClass.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const qualifiedDependentClass: Wu3QualifiedClassLeaves.Dependent<string, number>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
const qualifiedDependentClassValue: number = qualifiedDependentClass.value;
const qualifiedDependentClassWrong: string = qualifiedDependentClass.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const qualifiedPlainClass: Wu3QualifiedClassLeaves.Plain;
const qualifiedPlainClassValue: number = qualifiedPlainClass.value;
const qualifiedPlainClassWrong: string = qualifiedPlainClass.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
type QualifiedNonGenericClassLeaf = Wu3QualifiedClassLeaves.Plain<string>; // error[TK2315]: Type 'Plain' is not generic

interface QualifiedNestedClassHeritage extends Wu3QualifiedClassLeaves.Plain {
  own: boolean;
}
declare const qualifiedNestedClassHeritage: QualifiedNestedClassHeritage;
const qualifiedNestedClassHeritageValue: number = qualifiedNestedClassHeritage.value;
const qualifiedNestedClassHeritageWrong: string = qualifiedNestedClassHeritage.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

interface QualifiedConstrainedClassHeritage extends Wu3QualifiedClassLeaves.Constrained<number> { // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
  own: boolean;
}
declare const qualifiedConstrainedClassHeritage: QualifiedConstrainedClassHeritage;
const qualifiedConstrainedClassHeritageValue: number = qualifiedConstrainedClassHeritage.value;
const qualifiedConstrainedClassHeritageWrong: string = qualifiedConstrainedClassHeritage.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
