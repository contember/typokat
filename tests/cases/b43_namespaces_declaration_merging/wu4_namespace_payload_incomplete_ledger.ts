// WU4 incomplete ledger — tsc 6.0.3 --strict --noEmit --lib es5 --module commonjs:
// TS2451 x2 below; all other cases are clean.
// Each owner stays explicitly typed so only its attached namespace payload reaches a boundary.

function Wu4PayloadVariableOwner(): void {}
namespace Wu4PayloadVariableOwner { // incomplete[decl/module-declaration/self]
  export const inferredValue = Wu4PayloadVariableSource(); // incomplete[decl/variable-declaration/namespace-payload-inferred-initializer]
}
function Wu4PayloadVariableSource(): number {
  return 1;
}

declare const wu4PayloadAnnotationSource: number;
function Wu4PayloadAnnotationOwner(): void {}
namespace Wu4PayloadAnnotationOwner { // incomplete[decl/module-declaration/self]
  export const annotatedValue: typeof wu4PayloadAnnotationSource = 1; // incomplete[annotation-lower/type-query/typeof]
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
  export enum Mode { // incomplete[decl/enum-declaration/namespace-payload-unavailable]
    One,
  }
}

function Wu4PayloadImportOwner(): void {}
namespace Wu4PayloadImportOwner { // incomplete[decl/module-declaration/self]
  export import ForwardedOwner = Wu4PayloadVariableOwner; // incomplete[decl/import-equals/namespace-payload-unavailable]
}

function Wu4PayloadDuplicateOwner(): void {}
namespace Wu4PayloadDuplicateOwner { // incomplete[decl/module-declaration/self]
  export const duplicate: number = 1;
  export const duplicate: string = "two"; // incomplete[decl/variable-declaration/namespace-payload-duplicate-value]
}
