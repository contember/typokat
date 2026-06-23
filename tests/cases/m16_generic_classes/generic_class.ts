// M16 — generic classes: type parameters on a class scope the instance type and
// constructor. Instantiate via explicit args (`new Box<number>`) or infer from the
// constructor arguments (`new Box(5)`); `Box<T>` is usable as a type.

class Box<T> {
  value: T;
  constructor(v: T) {
    this.value = v;
  }
  get(): T {
    return this.value;
  }
}

const b = new Box<number>(1);
const n: number = b.get();   // ok
const bad: string = b.get(); // error[TK2322]
const e = new Box<number>("s"); // error[TK2345]

const inf = new Box(5);      // infer T = number from the constructor argument
const m: number = inf.get(); // ok
const m2: string = inf.get(); // error[TK2322]

const x: Box<number> = new Box<number>(1); // ok
const y: Box<string> = new Box<number>(1); // error[TK2322]
