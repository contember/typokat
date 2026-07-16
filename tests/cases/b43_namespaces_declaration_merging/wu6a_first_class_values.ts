// WU6A oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// The only diagnostics are the seven marked TS2322/TS2339/TS2345 demands.

namespace Wu6aRuntimeValue {
  export const count: number = 1;

  export function double(value: number): number {
    return value * 2;
  }

  export class Box {
    constructor(public value: number) {}
  }

  export namespace Nested {
    export let label: string = "nested";
  }

  const hidden: number = 1;

  export function bodyTraversal(): number {
    const wrong: string = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
    return hidden;
  }
}

namespace Wu6aRuntimeValue {
  export const reopened: string = "open";
}

namespace Wu6aEqualShapeLeft {
  export const value: number = 1;
}

namespace Wu6aEqualShapeRight {
  export const value: number = 1;
}

const wu6aRuntimeAlias = Wu6aRuntimeValue;
const wu6aRuntimeStaticRead: number = Wu6aRuntimeValue.count;
const wu6aRuntimeComputedRead: number = Wu6aRuntimeValue["count"];
const wu6aRuntimeAliasRead: string = wu6aRuntimeAlias.reopened;
const wu6aRuntimeNestedRead: string = Wu6aRuntimeValue.Nested.label;
const wu6aRuntimeCall: number = Wu6aRuntimeValue.double(1);
const wu6aRuntimeConstruct: number = new Wu6aRuntimeValue.Box(1).value;

function wu6aAcceptRuntimeRoot(value: { readonly count: number }): number {
  return value.count;
}

function wu6aReturnRuntimeRoot(): { readonly count: number } {
  return Wu6aRuntimeValue;
}

const wu6aRuntimePassed: number = wu6aAcceptRuntimeRoot(Wu6aRuntimeValue);
const wu6aRuntimeReturned: number = wu6aReturnRuntimeRoot().count;
const wu6aEqualShapeLeftRead: number = Wu6aEqualShapeLeft.value;
const wu6aEqualShapeRightRead: number = Wu6aEqualShapeRight.value;
Wu6aRuntimeValue.double("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
new Wu6aRuntimeValue.Box("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
Wu6aRuntimeValue.missing; // error[TK2339]: Property 'missing' does not exist
Wu6aRuntimeValue.hidden; // error[TK2339]: Property 'hidden' does not exist

declare namespace Wu6aAmbientValue {
  const count: number;
  function double(value: number): number;
  class Box {
    constructor(value: number);
    value: number;
  }
  namespace Nested {
    let label: string;
  }
  interface QualifiedType {
    value: number;
  }
}

declare namespace Wu6aAmbientValue {
  const reopened: string;
}

const wu6aAmbientAlias = Wu6aAmbientValue;
const wu6aAmbientStaticRead: number = Wu6aAmbientValue.count;
const wu6aAmbientComputedRead: number = Wu6aAmbientValue["count"];
const wu6aAmbientAliasRead: string = wu6aAmbientAlias.reopened;
const wu6aAmbientNestedRead: string = Wu6aAmbientValue.Nested.label;
const wu6aAmbientCall: number = Wu6aAmbientValue.double(1);
const wu6aAmbientConstruct: number = new Wu6aAmbientValue.Box(1).value;
const wu6aAmbientQualifiedType: Wu6aAmbientValue.QualifiedType = { value: 1 };
const wu6aAmbientPassed: number = wu6aAcceptRuntimeRoot(Wu6aAmbientValue);

function wu6aReturnAmbientRoot(): { readonly count: number } {
  return Wu6aAmbientValue;
}

const wu6aAmbientReturned: number = wu6aReturnAmbientRoot().count;
Wu6aAmbientValue.double("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
new Wu6aAmbientValue.Box("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
