// WU6A nested/dotted oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// Bottom-up value publication is clean; the two marked assignments prove value/type precision.

namespace Wu6aDotted.Chain {
  export const value: number = 1;
  export interface Shape {
    value: number;
  }
}

namespace Wu6aDotted.Chain {
  export let reopened: string = "open";
  export namespace Deep {
    export const flag: boolean = true;
  }
}

const wu6aDottedRootAlias = Wu6aDotted;
const wu6aDottedChainAlias = Wu6aDotted.Chain;
const wu6aDottedValue: number = wu6aDottedRootAlias.Chain.value;
const wu6aDottedComputed: string = wu6aDottedChainAlias["reopened"];
const wu6aDottedDeep: boolean = Wu6aDotted.Chain.Deep.flag;
const wu6aDottedQualifiedType: Wu6aDotted.Chain.Shape = { value: 1 };
const wu6aDottedWrongValue: string = Wu6aDotted.Chain.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu6aDottedWrongType: Wu6aDotted.Chain.Shape = { value: "bad" }; // error[TK2322]: Type 'string' is not assignable to type 'number'
