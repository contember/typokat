// M3 — call checking: arity (TK2554) and argument assignability (TK2345).

function takesNum(x: number): number { return x; }

const c1: number = takesNum(1); // ok
const c2 = takesNum(1, 2);      // error[TK2554]: Expected 1 arguments, but got 2
const c3 = takesNum();          // error[TK2554]: Expected 1 arguments, but got 0
const c4 = takesNum("s");       // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
