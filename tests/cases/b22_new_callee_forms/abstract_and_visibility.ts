// Backlog 22 — parenthesized and one-step const-aliased class values retain
// the direct class's abstract and constructor-accessibility checks.
// Cross-checked against tsc 6.0.3 --strict.

abstract class AbstractCtor {
  constructor() {}
}

new AbstractCtor(); // error[TK2511]: Cannot create an instance of an abstract class
new (AbstractCtor)(); // error[TK2511]: Cannot create an instance of an abstract class
const AbstractAlias = AbstractCtor;
new AbstractAlias(); // error[TK2511]: Cannot create an instance of an abstract class

class PrivateCtor {
  private constructor() {}
}

new PrivateCtor(); // error[TK2673]: Constructor of class 'PrivateCtor' is private and only accessible within the class declaration
new (PrivateCtor)(); // error[TK2673]: Constructor of class 'PrivateCtor' is private and only accessible within the class declaration
const PrivateAlias = PrivateCtor;
new PrivateAlias(); // error[TK2673]: Constructor of class 'PrivateCtor' is private and only accessible within the class declaration

class ProtectedCtor {
  protected constructor() {}
}

new ProtectedCtor(); // error[TK2674]: Constructor of class 'ProtectedCtor' is protected and only accessible within the class declaration
new (ProtectedCtor)(); // error[TK2674]: Constructor of class 'ProtectedCtor' is protected and only accessible within the class declaration
const ProtectedAlias = ProtectedCtor;
new ProtectedAlias(); // error[TK2674]: Constructor of class 'ProtectedCtor' is protected and only accessible within the class declaration

class PublicCtor {
  constructor() {}
}

new PublicCtor();
new (PublicCtor)();
const PublicAlias = PublicCtor;
new PublicAlias();

abstract class AbstractPrivateCtor {
  private constructor() {}
}

new (AbstractPrivateCtor)(); // error[TK2673]: Constructor of class 'AbstractPrivateCtor' is private and only accessible within the class declaration
const AbstractPrivateAlias = AbstractPrivateCtor;
new AbstractPrivateAlias(); // error[TK2673]: Constructor of class 'AbstractPrivateCtor' is private and only accessible within the class declaration
