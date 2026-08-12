enum EnumThenFunction { A } // error[TK2567]
function EnumThenFunction(): void {} // error[TK2567]

function FunctionThenEnum(): void {} // error[TK2567]
enum FunctionThenEnum { A } // error[TK2567]

enum EnumNamespaceFunction { A } // error[TK2567]
namespace EnumNamespaceFunction { // error[TK2434]
  export const tag = "tag";
}
function EnumNamespaceFunction(): void {} // error[TK2567]
const enfCall: void = EnumNamespaceFunction();
const enfTag: string = EnumNamespaceFunction.tag;
const enfType: EnumNamespaceFunction = 1; // error[TK2709]
const enfMember = EnumNamespaceFunction.A; // error[TK2339]

enum EnumFunctionNamespace { A } // error[TK2567]
function EnumFunctionNamespace(): void {} // error[TK2567]
namespace EnumFunctionNamespace {
  export const tag = "tag";
}
const efnCall: void = EnumFunctionNamespace();
const efnTag: string = EnumFunctionNamespace.tag;
const efnType: EnumFunctionNamespace = 1; // error[TK2709]
const efnMember = EnumFunctionNamespace.A; // error[TK2339]

namespace NamespaceEnumFunction { // error[TK2434] | error[TK2567]
  export const tag = "tag";
}
enum NamespaceEnumFunction { A } // error[TK2567]
function NamespaceEnumFunction(): void {} // error[TK2567]
const nefCall: void = NamespaceEnumFunction();
const nefTag: string = NamespaceEnumFunction.tag;
const nefType: NamespaceEnumFunction = 1; // error[TK2709]
const nefMember = NamespaceEnumFunction.A; // error[TK2339]

namespace NamespaceFunctionEnum { // error[TK2434] | error[TK2567]
  export const tag = "tag";
}
function NamespaceFunctionEnum(): void {} // error[TK2567]
enum NamespaceFunctionEnum { A } // error[TK2567]
const nfeCall: void = NamespaceFunctionEnum();
const nfeTag: string = NamespaceFunctionEnum.tag;
const nfeType: NamespaceFunctionEnum = 1; // error[TK2709]
const nfeMember = NamespaceFunctionEnum.A; // error[TK2339]

function FunctionEnumNamespace(): void {} // error[TK2567]
enum FunctionEnumNamespace { A } // error[TK2567]
namespace FunctionEnumNamespace {
  export const tag = "tag";
}
const fenCall: void = FunctionEnumNamespace();
const fenTag: string = FunctionEnumNamespace.tag;
const fenType: FunctionEnumNamespace = 1; // error[TK2709]
const fenMember = FunctionEnumNamespace.A; // error[TK2339]

function FunctionNamespaceEnum(): void {} // error[TK2567]
namespace FunctionNamespaceEnum { // error[TK2567]
  export const tag = "tag";
}
enum FunctionNamespaceEnum { A } // error[TK2567]
const fneCall: void = FunctionNamespaceEnum();
const fneTag: string = FunctionNamespaceEnum.tag;
const fneType: FunctionNamespaceEnum = 1; // error[TK2709]
const fneMember = FunctionNamespaceEnum.A; // error[TK2339]

const enum ConstMismatch { A } // error[TK2567]
enum ConstMismatch { B } // error[TK2567]
