// F1 / backlog 05 (WU3) - object type literal construct signatures make values
// constructable, and the constructed result has the signature's instance type.
// Cross-checked against tsc 6.0.3 --strict.

interface Box {
  value: number;
}

interface TextBox {
  value: string;
}

type ObjectCtor = {
  new (input: number): Box;
};

declare const objectCtor: ObjectCtor;

const objectBox: Box = new objectCtor(1);                    // ok - construct result is Box
const objectValue: number = new objectCtor(1).value;         // ok - member read comes from Box
const objectWrongBox: TextBox = new objectCtor(1);           // error[TK2322]
const objectBadValue: boolean = new objectCtor(1).value;     // error[TK2322]: Type 'number' is not assignable to type 'boolean'
new objectCtor();                                            // error[TK2554]: Expected 1 arguments, but got 0
new objectCtor(1, 2);                                        // error[TK2554]: Expected 1 arguments, but got 2
new objectCtor("bad");                                      // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

type OptionalObjectCtor = {
  new (input: number, label?: string): Box;
};

type RestObjectCtor = {
  new (input: number, ...labels: string[]): Box;
};

declare const optionalObjectCtor: OptionalObjectCtor;
declare const restObjectCtor: RestObjectCtor;

new optionalObjectCtor(1);
new optionalObjectCtor();                                     // error[TK2554]: Expected 1-2 arguments, but got 0
new optionalObjectCtor(1, "x", "extra");                     // error[TK2554]: Expected 1-2 arguments, but got 3
new optionalObjectCtor(1, 2);                                 // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
new restObjectCtor(1, "x");
new restObjectCtor();                                         // error[TK2555]: Expected at least 1 arguments, but got 0
new restObjectCtor(1, 2);                                     // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

declare const typedCtor: new (input: number, label?: string) => Box;
new typedCtor(1);
new typedCtor();                                              // error[TK2554]: Expected 1-2 arguments, but got 0
new typedCtor(1, "x", "extra");                              // error[TK2554]: Expected 1-2 arguments, but got 3

declare const typedRestCtor: new (input: number, ...labels: string[]) => Box;
new typedRestCtor(1, "x");
new typedRestCtor();                                          // error[TK2555]: Expected at least 1 arguments, but got 0
new typedRestCtor(1, 2);                                      // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
