// F1 / backlog 05 (WU2) - interface and object type literal call signatures
// make values callable, and the call result has the signature's return type.
// Cross-checked against tsc 6.0.3 --strict.

interface InterfaceCallable {
  (input: number): string;
}

type ObjectCallable = {
  (input: number): string;
};

declare const interfaceCallable: InterfaceCallable;
declare const objectCallable: ObjectCallable;

const interfaceResult: string = interfaceCallable(1);              // ok - interface call signature returns string
const objectResult: string = objectCallable(1);                    // ok - object type literal call signature returns string
const interfaceBadResult: number = interfaceCallable(1);           // error[TK2322]: Type 'string' is not assignable to type 'number'
const objectBadResult: boolean = objectCallable(1);                // error[TK2322]: Type 'string' is not assignable to type 'boolean'
interfaceCallable();                                               // error[TK2554]: Expected 1 arguments, but got 0
objectCallable(1, 2);                                              // error[TK2554]: Expected 1 arguments, but got 2
interfaceCallable("bad");                                         // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
objectCallable("bad");                                            // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

interface OptionalCallable {
  (input: number, label?: string): string;
}

type RestCallable = {
  (input: number, ...labels: string[]): void;
};

declare const optionalCallable: OptionalCallable;
declare const restCallable: RestCallable;

const optionalResult: string = optionalCallable(1);
optionalCallable();                                                // error[TK2554]: Expected 1-2 arguments, but got 0
optionalCallable(1, "x", "extra");                                // error[TK2554]: Expected 1-2 arguments, but got 3
optionalCallable(1, 2);                                            // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
restCallable(1, "x");
restCallable();                                                    // error[TK2555]: Expected at least 1 arguments, but got 0
restCallable(1, 2);                                                // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
