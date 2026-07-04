// b20 (backlog 20): TK2673 — a private constructor is constructable only lexically
// inside its declaring class (instance methods and statics both count).
// tsc 6.0.3 --strict cross-checked (sprint 2026-07-04 probes).

class PrivCtor {
  private constructor() {}
  static make(): PrivCtor { return new PrivCtor(); }
  clone(): PrivCtor { return new PrivCtor(); }
}

class Unrelated {
  m(): PrivCtor { return new PrivCtor(); } // error[TK2673]: Constructor of class 'PrivCtor' is private and only accessible within the class declaration
}

new PrivCtor(); // error[TK2673]: Constructor of class 'PrivCtor' is private and only accessible within the class declaration
