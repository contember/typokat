// Backlog 102 control. Fresh script-scope declarations consumed in the SAME file already
// resolve on the frozen library base; this fixture is the regression net that the cross-file
// fix must not disturb, and the non-permissive witness that each name carries a real type
// rather than an error/`any` recovery.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2322 x6 and nothing else.
interface B102SameShape {
  name: string;
}
declare var b102SameValue: number;
declare function b102SameFn(input: number): string;
declare namespace B102SameNamespace {
  const member: number;
}
class B102SameClass {
  value: number = 1;
}
type B102SameAlias = string;

declare const b102SameShapeValue: B102SameShape;
const sameShapeName: string = b102SameShapeValue.name;
const wrongSameShapeName: number = b102SameShapeValue.name; // error[TK2322]: Type 'string' is not assignable to type 'number'

const sameValue: number = b102SameValue;
const wrongSameValue: string = b102SameValue; // error[TK2322]: Type 'number' is not assignable to type 'string'

const sameFnResult: string = b102SameFn(1);
const wrongSameFnResult: number = b102SameFn(1); // error[TK2322]: Type 'string' is not assignable to type 'number'

const sameNamespaceMember: number = B102SameNamespace.member;
const wrongSameNamespaceMember: string = B102SameNamespace.member; // error[TK2322]: Type 'number' is not assignable to type 'string'

const sameClassValue: number = new B102SameClass().value;
const wrongSameClassValue: string = new B102SameClass().value; // error[TK2322]: Type 'number' is not assignable to type 'string'

const sameAlias: B102SameAlias = "text";
const wrongSameAlias: number = sameAlias; // error[TK2322]: Type 'string' is not assignable to type 'number'
