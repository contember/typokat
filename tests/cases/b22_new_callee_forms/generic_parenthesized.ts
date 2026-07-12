// Backlog 22 review amendment — parentheses are transparent around a direct
// generic class value; they must preserve both generic construction and class facts.

class PublicGeneric<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
}

const explicitOk: PublicGeneric<string> = new (PublicGeneric)<string>("ok");
const inferredOk: PublicGeneric<number> = new (PublicGeneric)(1);

abstract class AbstractGeneric<T> {
  constructor(value: T) {}
}

new (AbstractGeneric)<number>(1); // error[TK2511]: Cannot create an instance of an abstract class

class PrivateGeneric<T> {
  private constructor(value: T) {}
}

new (PrivateGeneric)<number>(1); // error[TK2673]

class ProtectedGeneric<T> {
  protected constructor(value: T) {}
}

new (ProtectedGeneric)<number>(1); // error[TK2674]
