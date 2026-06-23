// M12 — super(...) constructor calls are checked against the base constructor,
// and a subclass without its own constructor inherits the base's signature.

class Base {
  id: number;
  constructor(id: number) {
    this.id = id;
  }
}

class Ok extends Base {
  constructor() {
    super(1); // ok — Base constructor takes (id: number)
  }
}

class BadArity extends Base {
  constructor() {
    super(); // error[TK2554]: Expected 1 arguments, but got 0
  }
}

class BadArg extends Base {
  constructor() {
    super("s"); // error[TK2345]
  }
}

class Plain extends Base {}

const p = new Plain(5);     // ok — inherits Base(id: number)
const q = new Plain();      // error[TK2554]: Expected 1 arguments, but got 0
const r = new Plain("s");   // error[TK2345]
