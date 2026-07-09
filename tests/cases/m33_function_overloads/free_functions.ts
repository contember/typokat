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
