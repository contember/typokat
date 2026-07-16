// WU4 adversarial HOLD — tsc 6.0.3 --strict --noEmit --lib es5 --module commonjs:
// TS2394, TS2391 x4, TS2434, TS2769 x6, TS2339 x3, and TS2322 x18 below.

function Wu4PrivateVariableOwner(): void {}
namespace Wu4PrivateVariableOwner {
  const hiddenValue: number = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}
Wu4PrivateVariableOwner.hiddenValue; // error[TK2339]: Property 'hiddenValue' does not exist

class Wu4PrivateFunctionOwner {
  static existing: number = 1;
}
namespace Wu4PrivateFunctionOwner {
  function hiddenFunction(): number {
    return "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
  }
}
Wu4PrivateFunctionOwner.hiddenFunction; // error[TK2339]: Property 'hiddenFunction' does not exist

function Wu4PrivateClassOwner(): void {}
namespace Wu4PrivateClassOwner {
  export const tag: string = "private-class";
  class HiddenClass {
    field: number = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'

    method(): number {
      return "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
    }
  }
}
const wu4PrivateClassTag: string = Wu4PrivateClassOwner.tag;
const wu4PrivateClassTagWrong: number = Wu4PrivateClassOwner.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
Wu4PrivateClassOwner.HiddenClass; // error[TK2339]: Property 'HiddenClass' does not exist

function Wu4NamespaceOverloadOwner(): void {}
namespace Wu4NamespaceOverloadOwner {
  export function incompatible(value: number): number; // error[TK2394]
  export function incompatible(value: string): string;
  export function incompatible(value: boolean): boolean {
    return value;
  }

  export function missing(value: number): number;
  export function missing(value: string): string; // error[TK2391]

  export function valid(value: number): number;
  export function valid(value: string): string;
  export function valid(value: number | string): number | string {
    return value;
  }
}

const wu4IncompatibleNumber: number = Wu4NamespaceOverloadOwner.incompatible(1);
const wu4IncompatibleString: string = Wu4NamespaceOverloadOwner.incompatible("one");
Wu4NamespaceOverloadOwner.incompatible(true); // error[TK2769]
const wu4IncompatibleNumberWrong: string = Wu4NamespaceOverloadOwner.incompatible(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4IncompatibleStringWrong: number = Wu4NamespaceOverloadOwner.incompatible("one"); // error[TK2322]: Type 'string' is not assignable to type 'number'

const wu4MissingNumber: number = Wu4NamespaceOverloadOwner.missing(1);
const wu4MissingString: string = Wu4NamespaceOverloadOwner.missing("one");
Wu4NamespaceOverloadOwner.missing(true); // error[TK2769]
const wu4MissingNumberWrong: string = Wu4NamespaceOverloadOwner.missing(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4MissingStringWrong: number = Wu4NamespaceOverloadOwner.missing("one"); // error[TK2322]: Type 'string' is not assignable to type 'number'

const wu4ValidNumber: number = Wu4NamespaceOverloadOwner.valid(1);
const wu4ValidString: string = Wu4NamespaceOverloadOwner.valid("one");
Wu4NamespaceOverloadOwner.valid(true); // error[TK2769]
const wu4ValidNumberWrong: string = Wu4NamespaceOverloadOwner.valid(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4ValidStringWrong: number = Wu4NamespaceOverloadOwner.valid("one"); // error[TK2322]: Type 'string' is not assignable to type 'number'

function Wu4SingletonSignatureOwner(): void {}
namespace Wu4SingletonSignatureOwner {
  export function singleton(value: number): number; // error[TK2391]
}
const wu4SingletonNumber: number = Wu4SingletonSignatureOwner.singleton(1);
const wu4SingletonNumberWrong: string = Wu4SingletonSignatureOwner.singleton(1); // error[TK2322]: Type 'number' is not assignable to type 'string'

function Wu4SplitSignatureOwner(): void {}
namespace Wu4SplitSignatureOwner {
  export function split(value: number): number; // error[TK2391]
}
namespace Wu4SplitSignatureOwner {
  export function split(value: string): string;
  export function split(value: number | string): number | string {
    return value;
  }
}
const wu4SplitNumber: number = Wu4SplitSignatureOwner.split(1);
const wu4SplitString: string = Wu4SplitSignatureOwner.split("one");
Wu4SplitSignatureOwner.split(true); // error[TK2769]
const wu4SplitNumberWrong: string = Wu4SplitSignatureOwner.split(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4SplitStringWrong: number = Wu4SplitSignatureOwner.split("one"); // error[TK2322]: Type 'string' is not assignable to type 'number'

function Wu4NonConsecutiveOwner(value: number): number; // error[TK2391]
namespace Wu4NonConsecutiveOwner { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export const tag: string = "non-consecutive";
}
function Wu4NonConsecutiveOwner(value: number): number {
  return value;
}
const wu4NonConsecutiveNumber: number = Wu4NonConsecutiveOwner(1);
const wu4NonConsecutiveTag: string = Wu4NonConsecutiveOwner.tag;
Wu4NonConsecutiveOwner("bad"); // error[TK2769]
const wu4NonConsecutiveNumberWrong: string = Wu4NonConsecutiveOwner(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4NonConsecutiveTagWrong: number = Wu4NonConsecutiveOwner.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'

function Wu4PrivateParameterOwner(): void {}
namespace Wu4PrivateParameterOwner {
  export const tag: string = "private-parameter";

  function validate(x: number): number {
    return x;
  }

  class HiddenValidator {
    validate(x: number): number {
      return x;
    }
  }
}
const wu4PrivateParameterTag: string = Wu4PrivateParameterOwner.tag;

declare function Wu4AmbientDefaultOverloadOwner(): void;
declare namespace Wu4AmbientDefaultOverloadOwner {
  function g(value: number): number;
  function g(value: string): string;
}
const wu4AmbientDefaultNumber: number = Wu4AmbientDefaultOverloadOwner.g(1);
const wu4AmbientDefaultString: string = Wu4AmbientDefaultOverloadOwner.g("one");
Wu4AmbientDefaultOverloadOwner.g(true); // error[TK2769]
const wu4AmbientDefaultNumberWrong: string = Wu4AmbientDefaultOverloadOwner.g(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4AmbientDefaultStringWrong: number = Wu4AmbientDefaultOverloadOwner.g("one"); // error[TK2322]: Type 'string' is not assignable to type 'number'
