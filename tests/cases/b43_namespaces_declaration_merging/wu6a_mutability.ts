// WU6A mutability oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// Exact result: TS2540 x2 for ordinary/ambient const; every other assignment is clean.

namespace Wu6aMutability {
  export const fixed: number = 1;
  export let letValue: number = 1;
  export var varValue: number = 1;

  export function callable(value: number): number {
    return value;
  }

  export class Constructable {
    constructor(public value: number) {}
  }

  export namespace Nested {
    export let value: number = 1;
  }
}

class Wu6aReplacementConstructable {
  constructor(public value: number) {}
}

declare namespace Wu6aAmbientMutability {
  const fixed: number;
  let letValue: number;
  var varValue: number;
  function callable(value: number): number;
  class Constructable {
    constructor(value: number);
    value: number;
  }
  namespace Nested {
    let value: number;
  }
}

Wu6aMutability.fixed = 2; // error[TK2540]: Cannot assign to 'fixed' because it is a read-only property
Wu6aMutability.letValue = 2;
Wu6aMutability.varValue = 2;
Wu6aMutability.callable = (value: number): number => value + 1;
Wu6aMutability.Constructable = Wu6aReplacementConstructable;
Wu6aMutability.Nested = { value: 2 };
Wu6aMutability.Nested.value = 3;

Wu6aAmbientMutability.fixed = 2; // error[TK2540]: Cannot assign to 'fixed' because it is a read-only property
Wu6aAmbientMutability.letValue = 2;
Wu6aAmbientMutability.varValue = 2;
Wu6aAmbientMutability.callable = (value: number): number => value + 1;
Wu6aAmbientMutability.Constructable = Wu6aReplacementConstructable;
Wu6aAmbientMutability.Nested = { value: 2 };
Wu6aAmbientMutability.Nested.value = 3;
