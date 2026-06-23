// M14 — member-assignment targets are type-checked: `obj.prop = expr` checks that
// expr is assignable to the property's type. (Fills a gap deferred since M11 —
// member assignments were previously unchecked.)

class Box {
  v: number;
  constructor(v: number) {
    this.v = v; // ok — number to number
  }
}

const b = new Box(1);
b.v = 2;   // ok
b.v = "s"; // error[TK2322]

const o = { a: 1 };
o.a = 3;   // ok
o.a = "x"; // error[TK2322]
