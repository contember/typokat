// tsc 6.0.3 --strict: TS2702 x2, TS2503, TS2694 x4, and TS2713 x2 below.
// WU2 owns path topology/slot diagnostics and forward classification; leaf lowering stays in WU3.
type AliasRoot = { alias: true };
let aliasRoot: AliasRoot.Member; // error[TK2702]: 'AliasRoot' only refers to a type, but is being used as a namespace here

class ClassRoot {}
let classRoot: ClassRoot.Member; // error[TK2702]: 'ClassRoot' only refers to a type, but is being used as a namespace here

const ValueRoot = 1;
let valueRoot: ValueRoot.Member; // error[TK2503]: Cannot find namespace 'ValueRoot'

namespace TopologyRoot {
  export const ValueMiddle = 1;
  export interface TypeMiddle { type: true }
  export class ClassMiddle {}
  export namespace NamespaceLeaf {}
  export interface ParentLeaf { parent: true }
  export namespace Child {}
}

let missingIntermediate: TopologyRoot.Missing.Leaf; // error[TK2694]: Namespace 'TopologyRoot' has no exported member 'Missing'
let valueIntermediate: TopologyRoot.ValueMiddle.Leaf; // error[TK2694]: Namespace 'TopologyRoot' has no exported member 'ValueMiddle'
let typeIntermediate: TopologyRoot.TypeMiddle.Leaf; // error[TK2713]: Cannot access 'TypeMiddle.Leaf' because 'TypeMiddle' is a type, but not a namespace. Did you mean to retrieve the type of the property 'Leaf' in 'TypeMiddle' with 'TypeMiddle["Leaf"]'?
let classIntermediate: TopologyRoot.ClassMiddle.Leaf; // error[TK2713]: Cannot access 'ClassMiddle.Leaf' because 'ClassMiddle' is a type, but not a namespace. Did you mean to retrieve the type of the property 'Leaf' in 'ClassMiddle' with 'ClassMiddle["Leaf"]'?
let noParentFallback: TopologyRoot.Child.ParentLeaf; // error[TK2694]: Namespace 'TopologyRoot.Child' has no exported member 'ParentLeaf'
let namespaceOnlyLeaf: TopologyRoot.NamespaceLeaf; // error[TK2694]: Namespace 'TopologyRoot' has no exported member 'NamespaceLeaf'

let forwardReopening: ForwardRoot.Later;
declare namespace ForwardRoot {
  interface Earlier { earlier: true }
}
declare namespace ForwardRoot {
  interface Later { later: true }
}
