// F1 / backlog 05 (WU3) - single construct signatures participate in
// assignment compatibility without depending on ordinary function runtime
// constructability.
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

type ObjectCtor = {
  new (input: number): Box;
};

class BoxClass {
  value: number;

  constructor(input: number) {
    this.value = input;
  }
}

declare const declaredCtor: new (input: number) => Box;
declare const interfaceCtor: InterfaceCtor;
declare const objectCtor: ObjectCtor;

const classToInterface: InterfaceCtor = BoxClass;                         // ok - class constructor has a matching construct signature
const classToObject: ObjectCtor = BoxClass;                               // ok - class constructor has a matching construct signature
const declaredToInterface: InterfaceCtor = declaredCtor;                  // ok - constructor-typed value to construct-signature interface
const declaredToObject: ObjectCtor = declaredCtor;                        // ok - constructor-typed value to construct-signature object type
const interfaceToNewType: new (input: number) => Box = interfaceCtor;      // ok - construct-signature interface to constructor function type
const objectToNewType: new (input: number) => Box = objectCtor;            // ok - construct-signature object type to constructor function type

interface StringParamCtor {
  new (input: string): Box;
}

interface TextResultCtor {
  new (input: number): TextBox;
}

declare const stringParamCtor: StringParamCtor;
declare const textResultCtor: TextResultCtor;

const badParamCtor: InterfaceCtor = stringParamCtor;                      // error[TK2322]
const badResultCtor: InterfaceCtor = textResultCtor;                      // error[TK2322]
