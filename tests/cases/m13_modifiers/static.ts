// M13 — static members live on the class value (the "static side"), not on
// instances; instance members are not on the class value.

class Counter {
  static total: number;
  static reset(): void {}
  value: number;
  constructor() {
    this.value = 0;
  }
}

const t: number = Counter.total;   // ok — static field on the class
Counter.reset();                   // ok — static method
const bad: string = Counter.total; // error[TK2322]

const c = new Counter();
const v: number = c.value;         // ok — instance field
const x = c.total;                 // error[TK2339]: Property 'total' does not exist
// ^ static member is not on instances
const z = Counter.value;           // error[TK2339]: Property 'value' does not exist
// ^ instance member is not on the class value
