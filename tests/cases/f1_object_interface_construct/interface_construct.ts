// F1 / backlog 05 (WU3) - interface construct signatures make values
// constructable, and the constructed result has the signature's instance type.
// Cross-checked against tsc 6.0.3 --strict.

interface Box {
  value: number;
}

interface TextBox {
  value: string;
}

interface InterfaceCtor {
  new (input: number): Box;
}

declare const interfaceCtor: InterfaceCtor;

const interfaceBox: Box = new interfaceCtor(1);                 // ok - construct result is Box
const interfaceValue: number = new interfaceCtor(1).value;      // ok - member read comes from Box
const interfaceWrongBox: TextBox = new interfaceCtor(1);        // error[TK2322]
const interfaceBadValue: string = new interfaceCtor(1).value;   // error[TK2322]: Type 'number' is not assignable to type 'string'
new interfaceCtor();                                            // error[TK2554]: Expected 1 arguments, but got 0
new interfaceCtor(1, 2);                                        // error[TK2554]: Expected 1 arguments, but got 2
new interfaceCtor("bad");                                      // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

interface OptionalInterfaceCtor {
  new (input: number, label?: string): Box;
}

interface RestInterfaceCtor {
  new (input: number, ...labels: string[]): Box;
}

declare const optionalInterfaceCtor: OptionalInterfaceCtor;
declare const restInterfaceCtor: RestInterfaceCtor;

new optionalInterfaceCtor(1);
new optionalInterfaceCtor();                                    // error[TK2554]: Expected 1-2 arguments, but got 0
new optionalInterfaceCtor(1, "x", "extra");                    // error[TK2554]: Expected 1-2 arguments, but got 3
new optionalInterfaceCtor(1, 2);                                // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
new restInterfaceCtor(1, "x");
new restInterfaceCtor();                                        // error[TK2555]: Expected at least 1 arguments, but got 0
new restInterfaceCtor(1, 2);                                    // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
