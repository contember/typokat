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

class OptionalBase {
  constructor(id: number, label?: string) {}
}

class OptionalChild extends OptionalBase {
  constructor() {
    super(); // error[TK2554]: Expected 1-2 arguments, but got 0
  }
}

const inheritedOptionalOwnCtor = new OptionalChild(1); // error[TK2554]: Expected 0 arguments, but got 1

class OptionalPlain extends OptionalBase {}
const inheritedOptionalPlainOk = new OptionalPlain(1);
const inheritedOptionalFew = new OptionalPlain();                // error[TK2554]: Expected 1-2 arguments, but got 0
const inheritedOptionalMany = new OptionalPlain(1, "x", "extra"); // error[TK2554]: Expected 1-2 arguments, but got 3

class RestBase {
  constructor(id: number, ...labels: string[]) {}
}

class RestChildFew extends RestBase {
  constructor() {
    super(); // error[TK2555]: Expected at least 1 arguments, but got 0
  }
}

class RestChildBadArg extends RestBase {
  constructor() {
    super(1, 2); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
  }
}
