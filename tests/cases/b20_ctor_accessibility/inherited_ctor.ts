// b20: a derived class with NO own constructor inherits the base's constructor
// INCLUDING its visibility — the diagnostic names the DECLARING class (tsc 6.0.3).
// A derived class with its own public constructor is freely constructable.

class ProtBase {
  protected constructor() {}
}

class SubNoCtor extends ProtBase {}

class SubPublicCtor extends ProtBase {
  constructor() { super(); }
}

new SubNoCtor(); // error[TK2674]: Constructor of class 'ProtBase' is protected and only accessible within the class declaration
new SubPublicCtor();
new ProtBase(); // error[TK2674]: Constructor of class 'ProtBase' is protected and only accessible within the class declaration
