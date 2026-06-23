// M14 — readonly properties: assignable inside the declaring class's constructor,
// but not afterwards (other methods, external code).

class Point {
  readonly x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x; // ok — readonly is assignable in the constructor
    this.y = y;
  }
  reset(): void {
    this.x = 0; // error[TK2540]: Cannot assign to 'x' because it is a read-only property
    this.y = 0; // ok — mutable
  }
}

const p = new Point(1, 2);
p.y = 5; // ok
p.x = 5; // error[TK2540]: Cannot assign to 'x' because it is a read-only property
