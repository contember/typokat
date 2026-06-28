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
