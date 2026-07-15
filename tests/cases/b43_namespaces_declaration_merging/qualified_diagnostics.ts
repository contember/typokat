// tsc 6.0.3 --strict: TS2315, TS2344, TS2503 x2, TS2694 x2, TS2702, TS2707,
// and TS2749 below; roots and leaves resolve through their matching declaration slot.
namespace QualifiedNs {
  interface Hidden { hidden: true }
  export interface Box<T extends number = 1> { value: T }
  export interface Plain { value: number }
  export const OnlyValue = 1;
  export namespace Nested {
    export interface Item<T> { value: T }
  }
}

let missingRoot: MissingRoot.Member; // error[TK2503]: Cannot find namespace 'MissingRoot'
const ValueOnlyRoot = 1;
let valueOnlyRoot: ValueOnlyRoot.Member; // error[TK2503]: Cannot find namespace 'ValueOnlyRoot'
let missingMember: QualifiedNs.Missing; // error[TK2694]: Namespace 'QualifiedNs' has no exported member 'Missing'
let hiddenMember: QualifiedNs.Hidden; // error[TK2694]: Namespace 'QualifiedNs' has no exported member 'Hidden'
interface TypeOnlyRoot { local: true }
let typeOnlyRoot: TypeOnlyRoot.Member; // error[TK2702]: 'TypeOnlyRoot' only refers to a type, but is being used as a namespace here
let valueOnlyLeaf: QualifiedNs.OnlyValue; // error[TK2749]: 'QualifiedNs.OnlyValue' refers to a value, but is being used as a type here
let nonGenericLeaf: QualifiedNs.Plain<number>; // error[TK2315]: Type 'Plain' is not generic
let wrongArity: QualifiedNs.Box<1, 2>; // error[TK2707]: Generic type 'Box<T>' requires between 0 and 1 type arguments
let badConstraint: QualifiedNs.Box<string>; // error[TK2344]: Type 'string' does not satisfy the constraint 'number'
let nestedGeneric: QualifiedNs.Nested.Item<string> = { value: "ok" };
