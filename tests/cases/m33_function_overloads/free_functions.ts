// M33 - free function overload declarations and call resolution.
// Cross-checked against tsc 6.0.3 --strict.

function pickLiteral(x: 1): "one";
function pickLiteral(x: 2): "two";
function pickLiteral(x: 1 | 2): "one" | "two" { return "one"; }

const pickedOne: "one" = pickLiteral(1);
const pickedTwo: "two" = pickLiteral(2);
const wrongPick: "two" = pickLiteral(1); // error[TK2322]: Type '"one"' is not assignable to type '"two"'
pickLiteral(true); // error[TK2769]: No overload matches this call

function hiddenImplementation(x: number): number;
function hiddenImplementation(x: string): string;
function hiddenImplementation(x: number | string | boolean): number | string | boolean { return x; }

hiddenImplementation(true); // error[TK2769]: No overload matches this call

function overloadArity(x: number): number;
function overloadArity(x: number, y: string): string;
function overloadArity(x: number, y?: string): number | string { return y === undefined ? x : y; }

overloadArity(); // error[TK2554]: Expected 1-2 arguments, but got 0
overloadArity(1, "x", "extra"); // error[TK2554]: Expected 1-2 arguments, but got 3

function incompatibleImplementation(x: number): number; // error[TK2394]: not compatible with its implementation signature
function incompatibleImplementation(x: string): string;
function incompatibleImplementation(x: boolean): boolean { return x; }

export function exportedOverload(x: number): number;
export function exportedOverload(x: string): string;
export function exportedOverload(x: number | string): number | string { return x; }

const exportedNumber: number = exportedOverload(1);
const exportedString: string = exportedOverload("x");
exportedOverload(true); // error[TK2769]: No overload matches this call

declare function ambientOverload(x: number): number;
declare function ambientOverload(x: string): string;

const ambientNumber: number = ambientOverload(1);
const ambientString: string = ambientOverload("x");
ambientOverload(true); // error[TK2769]: No overload matches this call

function chooseExplicit<T extends string>(x: T): T;
function chooseExplicit<T extends number>(x: T): T;
function chooseExplicit<T extends string | number>(x: T): T { return x; }

const chooseExplicitWrong: string = chooseExplicit<number>(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
chooseExplicit<boolean>(true); // error[TK2344]: Type 'boolean' does not satisfy the constraint 'string'

function hiddenNonContiguous(x: number): number; // error[TK2391]: Function implementation is missing or not immediately following the declaration
const separator = 1;
function hiddenNonContiguous(x: number | string): number | string { return x; }
hiddenNonContiguous("s");
