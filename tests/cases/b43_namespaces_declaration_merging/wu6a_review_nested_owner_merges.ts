// WU6A adversarial oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// Legal nested function/class namespace owners retain their original callable/constructable value
// identity while the namespace contributes mutable static properties.

namespace Wu6aReviewNestedOwners {
  export function Callable(value: number): number {
    return value;
  }

  export namespace Callable {
    export let tag: string = "callable";
  }

  export class Constructable {
    constructor(public value: number) {}
  }

  export namespace Constructable {
    export let tag: string = "constructable";
  }
}

const wu6aReviewCallableResult: number = Wu6aReviewNestedOwners.Callable(1);
const wu6aReviewCallableTag: string = Wu6aReviewNestedOwners.Callable.tag;
const wu6aReviewCallableWrong: number = Wu6aReviewNestedOwners.Callable.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
Wu6aReviewNestedOwners.Callable.tag = "updated";

const wu6aReviewConstructed: number = new Wu6aReviewNestedOwners.Constructable(1).value;
const wu6aReviewConstructableTag: string = Wu6aReviewNestedOwners.Constructable.tag;
const wu6aReviewConstructableWrong: number = Wu6aReviewNestedOwners.Constructable.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
Wu6aReviewNestedOwners.Constructable.tag = "updated";
