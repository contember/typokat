// Const assertions retain non-widening provenance through declarations, containers,
// returns, and generic inference. Ordinary literals still widen.

let retainedLet = 1 as const;
retainedLet = 2; // error[TK2322]: Type '2' is not assignable to type '1'

let copiedSource = 1 as const;
let copiedValue = copiedSource;
copiedValue = 2; // error[TK2322]: Type '2' is not assignable to type '1'

const retainedObject = { value: 1 as const };
const exactObjectValue: 1 = retainedObject.value;
retainedObject.value = 2; // error[TK2322]: Type '2' is not assignable to type '1'

const retainedArray = [1 as const];
const exactArrayValue: 1 = retainedArray[0];

function retainedFunction() {
  return 1 as const;
}
const retainedArrow = () => "arrow" as const;
const exactFunctionReturn: 1 = retainedFunction();
const exactArrowReturn: "arrow" = retainedArrow();

function constIdentity<T>(value: T): T {
  return value;
}
const inferredLiteral = constIdentity(1 as const);
const exactInferredLiteral: 1 = inferredLiteral;

let ordinaryLiteral = 1;
ordinaryLiteral = 2;
