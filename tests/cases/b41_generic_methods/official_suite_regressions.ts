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

interface MultiEventMap {
  warn: [string];
  shardDisconnect: [DisconnectEvent, number];
}

interface DisconnectEvent {
  code: number;
  wasClean: boolean;
  reason: string;
}

interface MultiEventClient {
  on<K extends keyof MultiEventMap>(event: K, listener: (...args: MultiEventMap[K]) => void): void;
}

declare const multiEventClient: MultiEventClient;
declare function acceptsDisconnect(event: DisconnectEvent): void;
declare function acceptsShard(shard: number): void;

multiEventClient.on("shardDisconnect", (event, shard) => {
  acceptsDisconnect(event);
  acceptsShard(shard);
});
multiEventClient.on("shardDisconnect", event => acceptsDisconnect(event));

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
