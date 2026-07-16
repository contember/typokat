// WU4 — tsc 6.0.3 --strict --noEmit --lib es5 --module commonjs: TS2322 x4 below.
// Interface+namespace coexistence is type-only here; no namespace value parity is claimed.
interface Wu4ForwardInterfaceNamespace {
  instance: number;
}
namespace Wu4ForwardInterfaceNamespace {
  export interface Nested {
    enabled: boolean;
  }
}

declare const wu4ForwardInterfaceInstance: Wu4ForwardInterfaceNamespace;
declare const wu4ForwardInterfaceNested: Wu4ForwardInterfaceNamespace.Nested;
const wu4ForwardInterfaceInstanceWrong: string = wu4ForwardInterfaceInstance.instance; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4ForwardInterfaceNestedWrong: number = wu4ForwardInterfaceNested.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

namespace Wu4ReverseInterfaceNamespace {
  export interface Nested {
    enabled: boolean;
  }
}
interface Wu4ReverseInterfaceNamespace {
  instance: number;
}

declare const wu4ReverseInterfaceInstance: Wu4ReverseInterfaceNamespace;
declare const wu4ReverseInterfaceNested: Wu4ReverseInterfaceNamespace.Nested;
const wu4ReverseInterfaceInstanceWrong: string = wu4ReverseInterfaceInstance.instance; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4ReverseInterfaceNestedWrong: number = wu4ReverseInterfaceNested.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'
