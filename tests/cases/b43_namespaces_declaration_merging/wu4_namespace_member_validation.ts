// WU4 adversarial HOLD — tsc 6.0.3 --strict --noEmit --lib es5 --module commonjs:
// TS2394, TS2391, TS2769 x3, TS2339 x2, and TS2322 x8 below.

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
