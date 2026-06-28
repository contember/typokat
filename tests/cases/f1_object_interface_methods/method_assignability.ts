// F1 / backlog 05 (WU1) - method-bearing object/interface types are related
// through the function type stored in the named method property.
// Cross-checked against tsc 6.0.3 --strict.

interface NamedSource {
  run(value: number): string;
}

interface NamedTarget {
  run(input: number): string;
}

declare const namedSource: NamedSource;
const namedTarget: NamedTarget = namedSource; // ok - parameter names do not affect assignability

interface InterfaceParamNumber {
  run(x: number): string;
}

interface InterfaceParamString {
  run(x: string): string;
}

declare const interfaceParamString: InterfaceParamString;
const badInterfaceParam: InterfaceParamNumber = interfaceParamString; // error[TK2322]

type ObjectParamNumber = {
  run(x: number): string;
};

type ObjectParamString = {
  run(x: string): string;
};

declare const objectParamString: ObjectParamString;
const badObjectParam: ObjectParamNumber = objectParamString; // error[TK2322]

interface InterfaceReturnString {
  run(x: number): string;
}

interface InterfaceReturnNumber {
  run(x: number): number;
}

declare const interfaceReturnNumber: InterfaceReturnNumber;
const badInterfaceReturn: InterfaceReturnString = interfaceReturnNumber; // error[TK2322]

type ObjectReturnString = {
  run(x: number): string;
};

type ObjectReturnNumber = {
  run(x: number): number;
};

declare const objectReturnNumber: ObjectReturnNumber;
const badObjectReturn: ObjectReturnString = objectReturnNumber; // error[TK2322]
