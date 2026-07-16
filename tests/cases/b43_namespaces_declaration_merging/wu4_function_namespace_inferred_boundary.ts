// WU4 — tsc 6.0.3 --strict --noEmit --lib es5 --module commonjs:
// TS7023 x3, TS2769, and TS2322 x9 below. Typokat keeps the forward-demand and
// inferred-cycle boundaries explicitly incomplete under backlog 76.

function Wu4InferredAfterBody() {
  return 1;
}
namespace Wu4InferredAfterBody {
  export const tag: string = "after-body";
}

const wu4InferredAfterBodyReturn: number = Wu4InferredAfterBody();
const wu4InferredAfterBodyTag: string = Wu4InferredAfterBody.tag;
const wu4InferredAfterBodyReturnWrong: string = Wu4InferredAfterBody(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4InferredAfterBodyTagWrong: number = Wu4InferredAfterBody.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'

const wu4PendingBeforeBody: number = Wu4PendingBeforeBody(); // incomplete[expr-infer/call-expression/function-group-pending]
function Wu4PendingBeforeBody() {
  return 1;
}
namespace Wu4PendingBeforeBody {
  export const tag: string = "pending";
}

const wu4PendingAfterBodyReturn: number = Wu4PendingBeforeBody();
const wu4PendingAfterBodyTag: string = Wu4PendingBeforeBody.tag;
const wu4PendingAfterBodyReturnWrong: string = Wu4PendingBeforeBody(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4PendingAfterBodyTagWrong: number = Wu4PendingBeforeBody.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'

const wu4PendingIdentifier = Wu4PendingIdentifier; // incomplete[expr-infer/identifier/function-group-pending]
function Wu4PendingIdentifier() {
  return 1;
}
namespace Wu4PendingIdentifier {
  export const tag: string = "pending-identifier";
}
const wu4PendingIdentifierReturn: number = Wu4PendingIdentifier();
const wu4PendingIdentifierTag: string = Wu4PendingIdentifier.tag;

function Wu4InferredDependency() { // incomplete[decl/function-declaration/inferred-return-dependency]
  return Wu4InferredDependencyTarget();
}
namespace Wu4InferredDependency {
  export const tag: string = "dependency";
}
function Wu4InferredDependencyTarget() {
  return 1;
}
namespace Wu4InferredDependencyTarget {
  export const tag: string = "dependency-target";
}

function Wu4MutualDependencyA() { // incomplete[decl/function-declaration/inferred-return-dependency]
  return Wu4MutualDependencyB();
}
namespace Wu4MutualDependencyA {
  export const tag: string = "mutual-a";
}
function Wu4MutualDependencyB() { // incomplete[decl/function-declaration/inferred-return-dependency]
  return Wu4MutualDependencyA();
}
namespace Wu4MutualDependencyB {
  export const tag: string = "mutual-b";
}

// TS 6 infers `never` for an unconditional self-call; two recursive branches retain TS7023.
function Wu4InferredRecursive(toggle: boolean) { // incomplete[decl/function-declaration/inferred-return-cycle]
  return toggle ? Wu4InferredRecursive(false) : Wu4InferredRecursive(true);
}
namespace Wu4InferredRecursive {
  export const tag: string = "inferred-recursive";
}
const wu4InferredRecursiveDemand = Wu4InferredRecursive(true);

function Wu4AnnotatedRecursive(): number {
  return Wu4AnnotatedRecursive();
}
namespace Wu4AnnotatedRecursive {
  export const tag: string = "annotated-recursive";
}

const wu4AnnotatedRecursiveReturn: number = Wu4AnnotatedRecursive();
const wu4AnnotatedRecursiveTag: string = Wu4AnnotatedRecursive.tag;
const wu4AnnotatedRecursiveReturnWrong: string = Wu4AnnotatedRecursive(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4AnnotatedRecursiveTagWrong: number = Wu4AnnotatedRecursive.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'

function Wu4InferredOverload(value: number): number;
function Wu4InferredOverload(value: string): string;
function Wu4InferredOverload(value: number | string) {
  return value;
}
namespace Wu4InferredOverload {
  export const tag: string = "overload";
}

declare const wu4InferredOverloadUnion: number | string;
const wu4InferredOverloadNumber: number = Wu4InferredOverload(1);
const wu4InferredOverloadString: string = Wu4InferredOverload("ok");
const wu4InferredOverloadTag: string = Wu4InferredOverload.tag;
Wu4InferredOverload(wu4InferredOverloadUnion); // error[TK2769]
const wu4InferredOverloadNumberWrong: string = Wu4InferredOverload(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4InferredOverloadStringWrong: number = Wu4InferredOverload("ok"); // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4InferredOverloadTagWrong: number = Wu4InferredOverload.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
