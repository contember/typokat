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

class ShapedMethods {
  optional(input: number, label?: string): void {}
  rest(input: number, ...labels: string[]): void {}
}

const shaped = new ShapedMethods();
shaped.optional(1);
shaped.optional();                // error[TK2554]: Expected 1-2 arguments, but got 0
shaped.optional(1, 2);            // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
shaped.rest(1, "x");
shaped.rest();                    // error[TK2555]: Expected at least 1 arguments, but got 0
shaped.rest(1, 2);                // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
