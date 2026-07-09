// M33 - generic free-function overloads.
// Cross-checked against tsc 6.0.3 --strict.

function genericIdentity<T>(x: T[]): T[];
function genericIdentity<T>(x: T): T;
function genericIdentity<T>(x: T | T[]): T | T[] { return x; }

const genericArray: number[] = genericIdentity([1, 2]);
const genericScalar: number = genericIdentity(1);
const genericWrongArray: string = genericIdentity([1, 2]); // error[TK2322]: Type 'number[]' is not assignable to type 'string'
const genericWrongScalar: string[] = genericIdentity(1); // error[TK2322]: Type 'number' is not assignable to type 'string[]'

function genericBound<T extends number>(x: T): T;
function genericBound<T extends string>(x: T): T;
function genericBound<T extends number | string>(x: T): T { return x; }

genericBound(true); // error[TK2769]: No overload matches this call

interface GenericMethodOverloadsDeferred {
  // Method-level type parameters are backlog 41; this control must remain out of
  // the M33 acceptance surface.
  map<T>(x: T): T;
  map<T>(x: T[]): T[];
}
