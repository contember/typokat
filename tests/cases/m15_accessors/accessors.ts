// M15 — getters/setters. A get/set accessor is a property of the getter's return
// type; it is writable iff a setter exists (get-only behaves like readonly).

class Temperature {
  private _c: number;
  constructor(c: number) {
    this._c = c;
  }
  get celsius(): number {
    return this._c;
  }
  set celsius(v: number) {
    this._c = v;
  }
  get fahrenheit(): number {
    return this._c; // get-only — no setter
  }
}

const t = new Temperature(20);
const c: number = t.celsius;   // ok — getter type
const bad: string = t.celsius; // error[TK2322]
t.celsius = 25;                // ok — setter
t.celsius = "hot";             // error[TK2322]
t.fahrenheit = 100;            // error[TK2540]: Cannot assign to 'fahrenheit' because it is a read-only property
