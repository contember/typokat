// M15 — abstract classes cannot be instantiated; a concrete subclass can.
// Abstract methods (no body) are part of the instance type.

abstract class Shape {
  abstract area(): number;
  name: string;
  constructor(name: string) {
    this.name = name;
  }
}

const s = new Shape("x"); // error[TK2511]: Cannot create an instance of an abstract class

class Circle extends Shape {
  radius: number;
  constructor(r: number) {
    super("circle");
    this.radius = r;
  }
  area(): number {
    return this.radius;
  }
}

const c = new Circle(5);    // ok — concrete subclass
const a: number = c.area(); // ok — abstract method implemented
const n: string = c.name;   // ok — inherited field
const w: string = c.area(); // error[TK2322]
