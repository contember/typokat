// tsc 6.0.3 --strict --noEmit: TS2503 x11 plus the documented TS2322 recovery control below.
namespace Available {
  export interface Good { good: true }
}

enum DeferredEnum { A }

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

type MappedIndexedObject<Source> = {
  [Key in keyof Source]: MissingMapped.Object[Key]; // error[TK2503]: Cannot find namespace 'MissingMapped'
};
