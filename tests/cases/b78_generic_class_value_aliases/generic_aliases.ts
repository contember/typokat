// Backlog 78 — a one-step const alias of a generic class must preserve generic
// construction and class-keyed abstract/accessibility facts.

class PublicGeneric<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
}

const PublicAlias = PublicGeneric;
const explicitOk: PublicGeneric<string> = new PublicAlias<string>("ok");
const inferredOk: PublicGeneric<number> = new PublicAlias(1);
const inferredBad: PublicGeneric<string> = new PublicAlias(1); // error[TK2322]

abstract class AbstractGeneric<T> {
  constructor(value: T) {}
}

const AbstractAlias = AbstractGeneric;
new AbstractAlias<number>(1); // error[TK2511]: Cannot create an instance of an abstract class

class PrivateGeneric<T> {
  private constructor(value: T) {}
}

const PrivateAlias = PrivateGeneric;
new PrivateAlias<number>(1); // error[TK2673]

class ProtectedGeneric<T> {
  protected constructor(value: T) {}
}

const ProtectedAlias = ProtectedGeneric;
new ProtectedAlias<number>(1); // error[TK2674]
