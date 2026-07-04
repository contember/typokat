// b20: TK2674 — a protected constructor is constructable inside the declaring class
// or a subclass (constructing the base directly from the subclass is allowed).
// Argument checks still run where construction is permitted.

class ProtCtor {
  protected constructor(x: number) {}
  static make(): ProtCtor { return new ProtCtor(1); }
}

class SubProt extends ProtCtor {
  static create(): SubProt { return new SubProt(2); }
  static viaBase(): ProtCtor { return new ProtCtor(3); }
  static viaBaseBad(): ProtCtor { return new ProtCtor("s"); } // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
}

class Other {
  m(): ProtCtor { return new ProtCtor(4); } // error[TK2674]: Constructor of class 'ProtCtor' is protected and only accessible within the class declaration
}

new ProtCtor(5); // error[TK2674]: Constructor of class 'ProtCtor' is protected and only accessible within the class declaration
