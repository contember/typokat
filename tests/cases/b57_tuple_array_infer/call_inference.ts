// backlog 57, call-site half: a fresh array literal against a TUPLE parameter
// and a tuple value against an ARRAY parameter must both produce element
// candidates (call-site inference widens literals on fixing, so T = number).
declare function h<T>(t: [T, T]): T;
const h1: number = h([1, 2]);
const h3: string = h([1, 2]); // error[TK2322]: Type 'number' is not assignable to type 'string'
declare function j<T>(a: T[]): T;
declare const tt: [1, 2];
const j1: string = j(tt); // error[TK2322]: Type 'number' is not assignable to type 'string'
const j2: number = j(tt);
// Regression pin: fresh array literal vs array parameter already works.
declare function k<T>(a: T[]): T;
const k1: number = k([1, 2]);
const k2: string = k([1, 2]); // error[TK2322]: Type 'number' is not assignable to type 'string'
