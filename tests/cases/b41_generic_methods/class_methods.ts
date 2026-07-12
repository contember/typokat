// Backlog 41 — generic instance and static methods, including an outer generic
// class parameter substituted before a method-local parameter is instantiated.
// Cross-checked with tsc 6.0.3 --strict.

class Box<T> {
  constructor(readonly value: T) {}

  map<U>(transform: (value: T) => U): Box<U> {
    return new Box(transform(this.value));
  }
}

const numericBox = new Box(1);
const mappedBoolean: Box<boolean> = numericBox.map(value => value > 0);
const mappedWrong: Box<string> = numericBox.map(value => value); // error[TK2322]
const mappedExplicit: Box<string> = numericBox.map<string>(() => "mapped");
numericBox.map<string>(value => value); // error[TK2322]

class Factory {
  static of<U>(value: U): Box<U> {
    return new Box(value);
  }
}

const staticInferred: Box<number> = Factory.of(1);
const staticExplicit: Box<string> = Factory.of<string>("value");
Factory.of<string>(1); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
Factory.of<number, string>(1); // error[TK2558]: Expected 1 type arguments, but got 2

declare class Defaults<T> {
  value<U = T>(): U;
}

const defaultFromOuter: number = new Defaults<number>().value();
const defaultOverride: string = new Defaults<number>().value<string>();
const defaultMaterialized = new Defaults<number>().value();
const defaultWrong: boolean = defaultMaterialized; // error[TK2322]

class Shadow<T> {
  instance<T>(value: T): T {
    return value;
  }

  static staticValue<T>(value: T): T {
    return value;
  }
}

const shadowedInstance: string = new Shadow<number>().instance("value");
const shadowedInstanceWrong: number = new Shadow<number>().instance("value"); // error[TK2322]
const shadowedStatic: string = Shadow.staticValue("value");
const shadowedStaticWrong: number = Shadow.staticValue("value"); // error[TK2322]

class Parent {
  pair<U>(left: number, right: U): [number, U] {
    return [left, right];
  }
}

class Child extends Parent {}

const inheritedPair: [number, string] = new Child().pair(1, "value");
new Child().pair("value", 1); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
