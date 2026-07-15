// tsc 6.0.3 --strict --noEmit: clean; qualified leaves traverse exported type-only aliases.
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
