// WU6A unavailable oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// tsc reports only TS2451 x2 and TS2448+TS2454; all other declaration shapes are valid.
// Each incomplete marker is the exact non-43 owner retained while the whole namespace value is
// Unavailable. Root aliases below are direct-gate demands: they must never observe a partial object.

declare const wu6aTypeQuerySource: number;
namespace Wu6aTypeQueryUnavailable {
  export const value: typeof wu6aTypeQuerySource = 1; // incomplete[annotation-lower/type-query/typeof]
}
const wu6aTypeQueryWholeParent = Wu6aTypeQueryUnavailable;

declare function wu6aInferredInitializerSource(): number;
namespace Wu6aInferredInitializerUnavailable {
  export const value = wu6aInferredInitializerSource(); // incomplete[decl/variable-declaration/namespace-payload-inferred-initializer]
}
const wu6aInferredInitializerWholeParent = Wu6aInferredInitializerUnavailable;

namespace Wu6aInferredReturnUnavailable {
  export function value(input: number) { // incomplete[decl/function-declaration/namespace-payload-inferred-return]
    return input;
  }
}
const wu6aInferredReturnWholeParent = Wu6aInferredReturnUnavailable;

namespace Wu6aEnumUnavailable {
  export enum Mode { // incomplete[decl/enum-declaration/namespace-payload-unavailable]
    One,
  }
}
const wu6aEnumWholeParent = Wu6aEnumUnavailable;

namespace Wu6aImportSource {
  export const value: number = 1;
}
namespace Wu6aImportUnavailable {
  export import Forwarded = Wu6aImportSource; // incomplete[decl/import-equals/namespace-payload-unavailable]
}
const wu6aImportWholeParent = Wu6aImportUnavailable;

namespace Wu6aDuplicateUnavailable {
  export const value: number = 1;
  export const value: string = "two"; // incomplete[decl/variable-declaration/namespace-payload-duplicate-value]
}
const wu6aDuplicateWholeParent = Wu6aDuplicateUnavailable;

namespace Wu6aTdzUnavailable {
  export const before = after; // error[TK2448]: Block-scoped variable 'after' used before its declaration | error[TK2454]: Variable 'after' is used before being assigned | incomplete[decl/variable-declaration/namespace-payload-inferred-initializer]
  export const after: number = 1;
}
const wu6aTdzWholeParent = Wu6aTdzUnavailable;

namespace Wu6aClassStaticUnavailable {
  export class Box { // incomplete[decl/class-declaration/namespace-payload-static-cycle]
    static root = Wu6aClassStaticUnavailable;
  }
}
const wu6aClassStaticWholeParent = Wu6aClassStaticUnavailable;
