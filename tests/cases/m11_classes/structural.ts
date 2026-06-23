// M11 — class instance types are structural (no private members in this slice):
// an instance is interchangeable with a matching object type, both ways.

class Box {
  value: number;
  constructor(v: number) {
    this.value = v;
  }
}

const obj: { value: number } = new Box(1); // ok — instance -> object type
const fromObj: Box = { value: 1 };         // ok — object literal -> instance type
const bad: { value: string } = new Box(1); // error[TK2322]
const miss: Box = {};                      // error[TK2741]: Property 'value' is missing
