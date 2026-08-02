// tsc 6.0.3 --strict --target es2025: TS2397 + inner TS2322.
namespace globalThis { // error[TK2397]: Declaration name conflicts with built-in global identifier 'globalThis'.
  export const bad: number = "x"; // error[TK2322]: Type 'string' is not assignable to type 'number'
  export type Checked = { value: number };
}

namespace Outer {
  export namespace globalThis {
    export type Checked = { value: number };
  }
}
