// Backlog 41 WU4 regression — static members have no access to a class binder.
// Cross-checked with tsc 6.0.3 --strict.

interface OuterName {
  label: string;
}

class StaticLeak<T> {
  constructor(readonly value: T) {}

  instance(value: T): T { return value; }
  instanceConstraint<U extends T>(value: U): U { return value; }
  instanceDefault<U = T>(value: U): U { return value; }
  instanceBody(): T { const captured: T = this.value; return captured; }

  static field: T; // error[TK2302]: Static members cannot reference class type parameters
  static parameter(value: T): void {} // error[TK2302]: Static members cannot reference class type parameters
  static result(): T { throw 0; } // error[TK2302]: Static members cannot reference class type parameters
  static constraint<U extends T>(value: U): U { return value; } // error[TK2302]: Static members cannot reference class type parameters
  static defaulted<U = T>(value: U): U { return value; } // error[TK2302]: Static members cannot reference class type parameters
  static body(value: number): number { const captured: T = value; return value; } // error[TK2302]: Static members cannot reference class type parameters

  static own<U>(value: U): U { const captured: U = value; return captured; }
  static ownConstraint<U extends { id: number }>(value: U): U { return value; }
  static ownDefault<U = string>(value: U): U { return value; }
  static outerName(value: OuterName): OuterName { return value; }
}

const instance: number = new StaticLeak(1).instance(1);
const own: string = StaticLeak.own("ok");
const outerName: OuterName = StaticLeak.outerName({ label: "ok" });
