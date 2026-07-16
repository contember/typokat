// WU0 addendum — tsc 6.0.3 --strict oracle for approved class+interface+namespace merging.
class ClassFirstMerge {
  static existing: number = 1;
  own: number = 1;
  constructor(seed: number) {}
}
interface ClassFirstMerge {
  required: string;
  method(value: number): string;
  generic<T>(value: T): T;
  recursive: ClassFirstMerge;
}
namespace ClassFirstMerge {
  export const added: string = "class-first";
  export interface Options { enabled: boolean }
}

const classFirstConstructed = new ClassFirstMerge(1);
const classFirstRequired: string = classFirstConstructed.required;
const classFirstRequiredWrong: number = classFirstConstructed.required; // error[TK2322]: Type 'string' is not assignable to type 'number'
const classFirstMethod: string = classFirstConstructed.method(1);
const classFirstMethodWrong: number = classFirstConstructed.method(1); // error[TK2322]: Type 'string' is not assignable to type 'number'
const classFirstGeneric: string = classFirstConstructed.generic("ok");
const classFirstGenericWrong: number = classFirstConstructed.generic("bad"); // error[TK2322]: Type 'string' is not assignable to type 'number'
const classFirstRecursiveWrong: number = classFirstConstructed.recursive.required; // error[TK2322]: Type 'string' is not assignable to type 'number'
const classFirstExistingWrong: string = ClassFirstMerge.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const classFirstAddedWrong: number = ClassFirstMerge.added; // error[TK2322]: Type 'string' is not assignable to type 'number'
const classFirstOptions: ClassFirstMerge.Options = { enabled: true };
const classFirstOptionsWrong: number = classFirstOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'
new ClassFirstMerge("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
classFirstConstructed.missing; // error[TK2339]: Property 'missing' does not exist on type 'ClassFirstMerge'
ClassFirstMerge.required; // error[TK2339]: Property 'required' does not exist on type 'typeof ClassFirstMerge'
classFirstConstructed.existing; // error[TK2576]: Property 'existing' does not exist on type 'ClassFirstMerge'
const classFirstMissingRequired: ClassFirstMerge = { // error[TK2741]
  own: 1,
  method: () => "ok",
  generic: <T>(value: T) => value,
  recursive: classFirstConstructed,
};

interface InterfaceFirstMerge {
  required: string;
  method(value: number): string;
  generic<T>(value: T): T;
  recursive: InterfaceFirstMerge;
}
class InterfaceFirstMerge {
  static existing: number = 2;
  own: number = 2;
  constructor(seed: number) {}
}
namespace InterfaceFirstMerge {
  export const added: string = "interface-first";
  export interface Options { enabled: boolean }
}

const interfaceFirstConstructed = new InterfaceFirstMerge(2);
const interfaceFirstRequired: string = interfaceFirstConstructed.required;
const interfaceFirstRequiredWrong: number = interfaceFirstConstructed.required; // error[TK2322]: Type 'string' is not assignable to type 'number'
const interfaceFirstMethod: string = interfaceFirstConstructed.method(2);
const interfaceFirstMethodWrong: number = interfaceFirstConstructed.method(2); // error[TK2322]: Type 'string' is not assignable to type 'number'
const interfaceFirstGeneric: string = interfaceFirstConstructed.generic("ok");
const interfaceFirstGenericWrong: number = interfaceFirstConstructed.generic("bad"); // error[TK2322]: Type 'string' is not assignable to type 'number'
const interfaceFirstRecursiveWrong: number = interfaceFirstConstructed.recursive.required; // error[TK2322]: Type 'string' is not assignable to type 'number'
const interfaceFirstExistingWrong: string = InterfaceFirstMerge.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const interfaceFirstAddedWrong: number = InterfaceFirstMerge.added; // error[TK2322]: Type 'string' is not assignable to type 'number'
const interfaceFirstOptions: InterfaceFirstMerge.Options = { enabled: true };
const interfaceFirstOptionsWrong: number = interfaceFirstOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'
new InterfaceFirstMerge("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
interfaceFirstConstructed.missing; // error[TK2339]: Property 'missing' does not exist on type 'InterfaceFirstMerge'
InterfaceFirstMerge.required; // error[TK2339]: Property 'required' does not exist on type 'typeof InterfaceFirstMerge'
interfaceFirstConstructed.existing; // error[TK2576]: Property 'existing' does not exist on type 'InterfaceFirstMerge'
const interfaceFirstMissingRequired: InterfaceFirstMerge = { // error[TK2741]
  own: 2,
  method: () => "ok",
  generic: <T>(value: T) => value,
  recursive: interfaceFirstConstructed,
};
