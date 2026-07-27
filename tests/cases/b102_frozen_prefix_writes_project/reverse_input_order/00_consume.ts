// Backlog 102: input order must not matter. This file is checked BEFORE the file that declares
// the globals, exactly as `tsc` accepts. Every form is repeated here because the hoist
// reservations are per-form: a value slot filled only at its own module's execution would let
// the wrong-type witness below vanish instead of erroring.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports exactly TS2322 x6 here.
const lateValue: number = b102LateGlobal;
const wrongLateValue: string = b102LateGlobal; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const lateShape: B102LateShape;
const lateShapeName: string = lateShape.name;
const wrongLateShapeName: number = lateShape.name; // error[TK2322]: Type 'string' is not assignable to type 'number'

const lateFnResult: string = b102LateFn(1);
const wrongLateFnResult: number = b102LateFn(1); // error[TK2322]: Type 'string' is not assignable to type 'number'

const lateClassValue: number = new B102LateClass().value;
const wrongLateClassValue: string = new B102LateClass().value; // error[TK2322]: Type 'number' is not assignable to type 'string'

const lateNamespaceMember: number = B102LateNamespace.member;
const wrongLateNamespaceMember: string = B102LateNamespace.member; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const lateAlias: B102LateAlias;
const lateAliasValue: string = lateAlias;
const wrongLateAliasValue: number = lateAlias; // error[TK2322]: Type 'string' is not assignable to type 'number'
