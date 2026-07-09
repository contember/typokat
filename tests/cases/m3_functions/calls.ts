// M3 — call checking: arity (TK2554) and argument assignability (TK2345).

function takesNum(x: number): number { return x; }

const c1: number = takesNum(1); // ok
const c2 = takesNum(1, 2);      // error[TK2554]: Expected 1 arguments, but got 2
const c3 = takesNum();          // error[TK2554]: Expected 1 arguments, but got 0
const c4 = takesNum("s");       // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

function takesOptional(x: number, y?: string): void {}
takesOptional(1);
takesOptional(1, "s");
takesOptional();                // error[TK2554]: Expected 1-2 arguments, but got 0
takesOptional(1, "s", "extra"); // error[TK2554]: Expected 1-2 arguments, but got 3
takesOptional(1, 2);            // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

function takesDefault(x: number, y: string = "s"): void {}
takesDefault(1);
takesDefault();                 // error[TK2554]: Expected 1-2 arguments, but got 0
takesDefault(1, 2);             // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
function badDefault(x: number = "s"): void {} // error[TK2322]: Type 'string' is not assignable to type 'number'

function takesRest(x: number, ...parts: string[]): void {}
takesRest(1);
takesRest(1, "a", "b");
takesRest();                    // error[TK2555]: Expected at least 1 arguments, but got 0
takesRest(1, "a", 2);           // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

const optionalExpr = function (x: number, y?: string): void {};
optionalExpr(1);
optionalExpr();                 // error[TK2554]: Expected 1-2 arguments, but got 0

const restArrow = (x: number, ...parts: string[]): void => {};
restArrow(1, "a");
restArrow();                    // error[TK2555]: Expected at least 1 arguments, but got 0
restArrow(1, 2);                // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

declare const typedOptional: (x: number, y?: string) => void;
typedOptional(1);
typedOptional();                // error[TK2554]: Expected 1-2 arguments, but got 0
typedOptional(1, 2);            // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

declare const typedRest: (x: number, ...parts: string[]) => void;
typedRest(1, "a");
typedRest();                    // error[TK2555]: Expected at least 1 arguments, but got 0
typedRest(1, 2);                // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
