// WU4 incomplete ledger — tsc 6.0.3 --strict --noEmit --lib es5 --module commonjs: clean.
// Each owner stays explicitly typed so only its attached namespace payload reaches a boundary.

function Wu4PayloadVariableOwner(): void {}
namespace Wu4PayloadVariableOwner { // incomplete[decl/module-declaration/self]
  export const inferredValue = Wu4PayloadVariableSource(); // incomplete[decl/variable-declaration/namespace-payload-inferred-initializer]
}
function Wu4PayloadVariableSource(): number {
  return 1;
}

function Wu4PayloadFunctionOwner(): void {}
namespace Wu4PayloadFunctionOwner { // incomplete[decl/module-declaration/self]
  export function inferredReturn(value: number) { // incomplete[decl/function-declaration/namespace-payload-inferred-return]
    return value;
  }
}

function Wu4PayloadClassOwner(): void {}
namespace Wu4PayloadClassOwner { // incomplete[decl/module-declaration/self]
  export class ExportedClass { // incomplete[decl/class-declaration/namespace-payload-static-cycle]
    static value: number = 1;
  }
}

function Wu4PayloadEnumOwner(): void {}
namespace Wu4PayloadEnumOwner { // incomplete[decl/module-declaration/self]
  export enum Mode { // incomplete[decl/module-declaration/attached-value-unavailable]
    One,
  }
}

function Wu4PayloadImportOwner(): void {}
namespace Wu4PayloadImportOwner { // incomplete[decl/module-declaration/self]
  export import ForwardedOwner = Wu4PayloadVariableOwner; // incomplete[decl/module-declaration/attached-value-unavailable]
}
