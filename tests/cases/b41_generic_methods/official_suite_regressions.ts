// Backlog 41 WU5 regression corpus from the official TypeScript suite.
// Every marker is cross-checked with tsc 6.0.3 --strict.

interface EventMap {
  pair: [number, string];
}

interface EventClient {
  on<K extends keyof EventMap>(event: K, listener: (...args: EventMap[K]) => void): void;
}

declare const eventClient: EventClient;
declare function acceptsString(value: string): number;

eventClient.on("pair", (first, second) => acceptsString(second));

declare function tupleCallbacks(callbacks: [(value: number) => number, ...((value: string) => number)[]]): void;

tupleCallbacks([
  value => value * 2,
  value => acceptsString(value),
  value => acceptsString(value),
]);

declare function returnsNumber(value: number): number;
declare function callbackResult(callback: (value: number) => number): (value: number) => number;

callbackResult(value => returnsNumber(value));
callbackResult((value: number) => "wrong"); // error[TK2345]: Argument of type '(value: number) => string' is not assignable to parameter of type '(value: number) => number'

interface Base {
  base: string;
}

interface Derived extends Base {
  derived: string;
}

declare let constrainedConstructor: new <T extends Base, U extends Derived>(value: new (input: T) => U) => T;
declare let fixedConstructor: new <T extends Base>(value: new (input: T) => Derived) => T;

constrainedConstructor = fixedConstructor;
fixedConstructor = constrainedConstructor;

interface ConstructorBase {
  base: string;
}

interface ConstructorDerived extends ConstructorBase {
  derived: string;
}

interface ConstructorDerived2 extends ConstructorDerived {
  derived2: string;
}

declare let overloadedConstructors: {
  new (factory: {
    new <T extends ConstructorDerived>(value: T): T;
    new <T extends ConstructorBase>(value: T): T;
  }): unknown[];
  new (factory: {
    new <T extends ConstructorDerived2>(value: T): T;
    new <T extends ConstructorBase>(value: T): T;
  }): unknown[];
};
declare let genericConstructorFactory: new (
  factory: new <T>(value: T) => T,
) => unknown[];

overloadedConstructors = genericConstructorFactory;
genericConstructorFactory = overloadedConstructors;

declare let incompatibleConstructorResult: new <T extends Base>(
  value: new (input: T) => Derived,
) => string;
incompatibleConstructorResult = fixedConstructor; // error[TK2322]: not assignable
