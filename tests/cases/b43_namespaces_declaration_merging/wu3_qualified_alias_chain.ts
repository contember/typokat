// tsc 6.0.3 --strict --noEmit: qualified leaves traverse exported type-only aliases.
declare namespace Wu3QualifiedAliasChain {
  namespace HiddenMiddle {
    namespace HiddenInner {
      export interface Leaf { leaf: true }
    }
    export { type HiddenInner as PublicInner };
  }
  export { type HiddenMiddle as PublicMiddle };
}

type QualifiedAliasChainLeaf = Wu3QualifiedAliasChain.PublicMiddle.PublicInner.Leaf;

namespace QualifiedObjectAlias {
  export type Alias = { x: number };
}

interface QualifiedObjectDerived extends QualifiedObjectAlias.Alias {}
declare const qualifiedObject: QualifiedObjectDerived;
const qualifiedObjectGood: number = qualifiedObject.x;
const qualifiedObjectBad: string = qualifiedObject.x; // error[TK2322]: Type 'number' is not assignable to type 'string'
