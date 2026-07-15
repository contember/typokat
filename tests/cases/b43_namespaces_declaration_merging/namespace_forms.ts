// tsc 6.0.3 --strict: TS2322 x2 and TS2741 below; namespace forms are otherwise clean.
namespace IdentifierNs {
  export interface Item { item: number }
}

namespace NestedNs {
  export namespace Inner {
    export interface Leaf<T> { value: T }
  }
}

namespace DottedNs.Inner {
  export interface Leaf<T> { value: T }
}

namespace ReopenedNs {
  export interface First { first: number }
}
namespace ReopenedNs {
  export interface Second { second: string }
}

namespace InterfaceMergeNs {
  export interface Merged { first: number }
  export interface Merged { second: string }
}
namespace InterfaceMergeNs {
  export interface Merged { third: boolean }
}

namespace HeritageNs {
  export interface Base { base: number }
  export interface Base { merged: string }
}
interface QualifiedDerived extends HeritageNs.Base { own: boolean }

const identifierUse: IdentifierNs.Item = { item: 1 };
const nestedUse: NestedNs.Inner.Leaf<string> = { value: "ok" };
const dottedUse: DottedNs.Inner.Leaf<number> = { value: 1 };
const reopenedFirst: ReopenedNs.First = { first: 1 };
const reopenedSecond: ReopenedNs.Second = { second: "ok" };
const namespaceMerged: InterfaceMergeNs.Merged = { first: 1, second: "ok", third: true };
const namespaceMergedWrong: number = namespaceMerged.second; // error[TK2322]: Type 'string' is not assignable to type 'number'
const qualifiedDerived: QualifiedDerived = { base: 1, merged: "ok", own: true };
const qualifiedInheritedWrong: boolean = qualifiedDerived.base; // error[TK2322]: Type 'number' is not assignable to type 'boolean'
const qualifiedHeritageMissing: QualifiedDerived = { base: 1, own: true }; // error[TK2741]
