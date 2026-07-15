// tsc 6.0.3 --strict: TS2769 and TS2322 x8 below; namespaces follow declarations they augment.
interface InterfacePair { instance: number }
namespace InterfacePair {
  export interface Options { enabled: boolean }
}
let interfaceInstance: InterfacePair = { instance: 1 };
let interfaceOptions: InterfacePair.Options = { enabled: true };
const interfaceInstanceWrong: string = interfaceInstance.instance; // error[TK2322]: Type 'number' is not assignable to type 'string'
const interfaceOptionsWrong: number = interfaceOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

function FunctionPair(value: number): number;
function FunctionPair(value: string): string;
function FunctionPair(value: number | string): number | string { return value; }
namespace FunctionPair {
  export const tag = "function";
  export interface Options { enabled: boolean }
}
const functionCall: number = FunctionPair(1);
const functionTag: string = FunctionPair.tag;
let functionOptions: FunctionPair.Options = { enabled: true };
FunctionPair(true); // error[TK2769]
const functionReturnWrong: string = FunctionPair(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const functionTagWrong: number = FunctionPair.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'

class ClassPair {
  static existing = 1;
  private identity = 1;
  instance = 1;
}
namespace ClassPair {
  export const tag = "class";
  export interface Options { enabled: boolean }
}
const classInstance: ClassPair = new ClassPair();
const classTag: string = ClassPair.tag;
let classOptions: ClassPair.Options = { enabled: true };
const classExisting: number = ClassPair.existing;
const classTagWrong: number = ClassPair.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const classExistingWrong: string = ClassPair.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const classInstanceWrong: { instance: string } = new ClassPair(); // error[TK2322]

class ForeignClassPair {
  private identity = 1;
  instance = 1;
}
const classNominalWrong: ClassPair = new ForeignClassPair(); // error[TK2322]
