// WU0 addendum — strict-tsc namespace placement around class/interface merges.
class ClassNamespaceInterface { instance = 1; }
namespace ClassNamespaceInterface {
  export const added = "class-namespace-interface";
}
interface ClassNamespaceInterface { augmented: string }
const classNamespaceInterface = new ClassNamespaceInterface();
const classNamespaceInterfaceWrong: number = classNamespaceInterface.augmented; // error[TK2322]: Type 'string' is not assignable to type 'number'
const classNamespaceStaticWrong: number = ClassNamespaceInterface.added; // error[TK2322]: Type 'string' is not assignable to type 'number'

interface InterfaceNamespaceClass { augmented: string }
namespace InterfaceNamespaceClass { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export const added = "interface-namespace-class";
}
class InterfaceNamespaceClass { instance = 1; }

namespace NamespaceInterfaceClass { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export const added = "namespace-interface-class";
}
interface NamespaceInterfaceClass { augmented: string }
class NamespaceInterfaceClass { instance = 1; }

declare namespace AmbientReverseCombination {
  const added: string;
  interface Options { enabled: boolean }
}
interface AmbientReverseCombination { augmented: string }
declare class AmbientReverseCombination { instance: number }
const ambientReverseCombination = new AmbientReverseCombination();
const ambientReverseInstanceWrong: number = ambientReverseCombination.augmented; // error[TK2322]: Type 'string' is not assignable to type 'number'
const ambientReverseStaticWrong: number = AmbientReverseCombination.added; // error[TK2322]: Type 'string' is not assignable to type 'number'
const ambientReverseOptions: AmbientReverseCombination.Options = { enabled: true };
