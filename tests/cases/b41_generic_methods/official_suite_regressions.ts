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

declare function takesString(value: string): void;
declare function withNumberCallback(callback: (value: number) => void): void;

withNumberCallback(value => takesString(value)); // error[TK2345]
eventClient.on("pair", first => takesString(first)); // error[TK2345]

interface TupleEvents {
  pair: [number, string];
}

declare function tupleOn<K extends keyof TupleEvents>(
  event: K,
  callbacks: [(...args: TupleEvents[K]) => void],
): void;

tupleOn("pair", [first => takesString(first)]); // error[TK2345]
tupleOn("pair", [(_first, second) => takesString(second)]);

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

declare let sameSlotsConstructor: new <T>(
  first: { foo: T },
  second: { foo: T; bar: T },
) => Base;
declare let splitSlotsConstructor: new <T, U>(
  first: { foo: T },
  second: { foo: U; bar: U },
) => Base;

sameSlotsConstructor = splitSlotsConstructor;
splitSlotsConstructor = sameSlotsConstructor; // error[TK2322]: not assignable

declare let samePairConstructor: new <T>(value: { a: T; b: T }) => T[];
declare let splitPairConstructor: new <U, V>(value: { a: U; b: V }) => U[];

samePairConstructor = splitPairConstructor;
splitPairConstructor = samePairConstructor; // error[TK2322]: not assignable

declare let broaderConstructor: new <T extends Base, U extends Base>(
  value: new (input: T) => U,
) => T;

constrainedConstructor = broaderConstructor;
broaderConstructor = constrainedConstructor; // error[TK2322]: not assignable

declare let overloadedGenericConstructors: {
  new <T extends Derived>(factory: new (value: T) => T): T[];
  new <T extends Base>(factory: new (value: T) => T): T[];
};
declare let genericOuterConstructor: new <T>(factory: new (value: T) => T) => T[];

overloadedGenericConstructors = genericOuterConstructor;
genericOuterConstructor = overloadedGenericConstructors;

interface ConstructorSlots {
  same: new <T>(first: { foo: T }, second: { foo: T; bar: T }) => Base;
}

declare let constructorSlots: ConstructorSlots;
constructorSlots.same = splitSlotsConstructor;
splitSlotsConstructor = constructorSlots.same; // error[TK2322]: not assignable

declare let callable: (value: number) => number;
declare let callableCopy: (value: number) => number;
declare let constructable: new (value: number) => number;
declare let constructableCopy: new (value: number) => number;

callable = callableCopy;
constructable = constructableCopy;
callable = constructable; // error[TK2322]: not assignable
constructable = callable; // error[TK2322]: not assignable

declare let genericCallable: <T>(value: T) => T;
declare let genericCallableCopy: <T>(value: T) => T;
declare let genericConstructable: new <T>(value: T) => T;
declare let genericConstructableCopy: new <T>(value: T) => T;

genericCallable = genericCallableCopy;
genericConstructable = genericConstructableCopy;
genericCallable = genericConstructable; // error[TK2322]: not assignable
genericConstructable = genericCallable; // error[TK2322]: not assignable

declare let oneBinderConstructor: new <T>(value: T) => T;
declare let unusedTargetBinderConstructor: new <T, U>(value: T) => T;

unusedTargetBinderConstructor = oneBinderConstructor;
oneBinderConstructor = unusedTargetBinderConstructor;

declare let derivedReturnConstructor: new <T, U extends Derived>(value: T) => U;
declare let baseReturnConstructor: new <T>(value: T) => Base;

baseReturnConstructor = derivedReturnConstructor;
derivedReturnConstructor = baseReturnConstructor; // error[TK2322]: not assignable
