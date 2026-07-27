// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports exactly TS2322 x4 here — one per
// declaration form. Each pair is a clean read plus a wrong-type witness, so an `any`/error-type
// recovery cannot pass this file.
const crossFnResult: string = b102CrossFn(1);
const wrongCrossFnResult: number = b102CrossFn(1); // error[TK2322]: Type 'string' is not assignable to type 'number'

const crossNamespaceMember: number = B102CrossNamespace.member;
const wrongCrossNamespaceMember: string = B102CrossNamespace.member; // error[TK2322]: Type 'number' is not assignable to type 'string'

const crossClassValue: number = new B102CrossClass().value;
const wrongCrossClassValue: string = new B102CrossClass().value; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const crossAlias: B102CrossAlias;
const crossAliasValue: string = crossAlias;
const wrongCrossAliasValue: number = crossAlias; // error[TK2322]: Type 'string' is not assignable to type 'number'
