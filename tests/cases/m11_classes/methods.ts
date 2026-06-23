// M11 — methods + `this` (resolves to the instance type inside method bodies).

class Counter {
  count: number;
  constructor() {
    this.count = 0;
  }
  increment(by: number): number {
    return this.count + by; // this.count : number
  }
  bad(): string {
    return this.count; // error[TK2322]
    // ^ this.count is number, declared return is string
  }
}

const c = new Counter();
const n: number = c.increment(1); // ok
const m: string = c.increment(1); // error[TK2322]
const w = c.increment("s");       // error[TK2345]
