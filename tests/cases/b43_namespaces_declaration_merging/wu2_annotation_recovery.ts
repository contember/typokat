// tsc 6.0.3 --strict --noEmit: TS2394, TS2503 x13, plus the documented TS2314/TS2322
// recovery controls below.
namespace Available {
  export interface Good { good: true }
}

enum DeferredEnum { A } // incomplete[decl/enum-declaration/self]

function mixedOverload(value: DeferredEnum.A): number; // incomplete[annotation-lower/type-name/qualified-enum]
function mixedOverload(value: string): string; // error[TK2394]: This overload signature is not compatible with its implementation signature
function mixedOverload(value: number): number { return value; }

type GenericSiblingTraversal = <
  First extends DeferredEnum.A = MissingGeneric.FirstDefault, // incomplete[annotation-lower/type-name/qualified-enum] | error[TK2503]: Cannot find namespace 'MissingGeneric'
  Second extends MissingGeneric.SecondConstraint = MissingGeneric.SecondDefault, // error[TK2503]: Cannot find namespace 'MissingGeneric' | error[TK2503]: Cannot find namespace 'MissingGeneric'
>(
  this: MissingGeneric.ThisType, // error[TK2503]: Cannot find namespace 'MissingGeneric'
  value: MissingGeneric.Parameter, // error[TK2503]: Cannot find namespace 'MissingGeneric'
) => MissingGeneric.Return; // error[TK2503]: Cannot find namespace 'MissingGeneric'

// tsc reports TS2322 on the assignment. typokat must withhold the incomplete callable and
// suppress that cascade until the qualified enum endpoint is modelled by backlog 42.
type UnavailableCallable = <T extends DeferredEnum.A = DeferredEnum.A>() => T; // incomplete[annotation-lower/type-name/qualified-enum] | incomplete[annotation-lower/type-name/qualified-enum]
declare const unavailableCallable: UnavailableCallable;
const unavailableCallableResult = unavailableCallable();
const unavailableCallableMustStayOpaque: never = unavailableCallableResult;

type InterleavedOverload = {
  method(value: MissingOverload.First): void; // error[TK2503]: Cannot find namespace 'MissingOverload'
  middle: MissingOverload.Middle; // error[TK2503]: Cannot find namespace 'MissingOverload'
  method(value: MissingOverload.Last): void; // error[TK2503]: Cannot find namespace 'MissingOverload'
};

interface ResolvedPair<Left, Right> {
  left: Left;
  right: Right;
}
type ResolvedGenericArguments = ResolvedPair<Available.Good, MissingArguments.Second>; // incomplete[annotation-lower/type-name/qualified-name] | error[TK2503]: Cannot find namespace 'MissingArguments'

// tsc additionally reports TS2314 for Array's arity. That arity check remains WU3-owned;
// WU2 must still visit both unavailable arguments instead of short-circuiting on the first.
type UnavailableGenericArguments = Array<MissingArguments.First, AlsoMissingArguments.Second>; // error[TK2503]: Cannot find namespace 'MissingArguments' | error[TK2503]: Cannot find namespace 'AlsoMissingArguments'

type MappedIndexedObject<Source> = {
  [Key in keyof Source]: MissingMapped.Object[Key]; // error[TK2503]: Cannot find namespace 'MissingMapped'
};
