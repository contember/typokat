// Target flow reduction differs from widening the assignment expression's result.

let ordinaryTarget: 1 | 2 = 1;
ordinaryTarget = 2;
let ordinaryCopy = ordinaryTarget;
ordinaryCopy = 3; // error[TK2322]

let resultTarget: 1 | 2 = 1;
let assignmentResult = (resultTarget = 2);
assignmentResult = 3;

let invalidSource = 1 as const;
invalidSource = 2; // error[TK2322]
let invalidCopy = invalidSource;
invalidCopy = 3; // error[TK2322]

declare const reductionCondition: boolean;
let conditionalTarget: 1 | 2 = 1;
conditionalTarget = reductionCondition ? (1 as const) : (2 as const);
let conditionalCopy = conditionalTarget;
conditionalCopy = 3; // error[TK2322]

let logicalTarget: false | 1 = false;
logicalTarget = reductionCondition && (1 as const);
let logicalCopy = logicalTarget;
logicalCopy = 2; // error[TK2322]

function exactFlowReturn() {
  return 1 as const;
}
const flowHolder = { value: 1 as const };
let fromCall: 1 | 2 = 2;
fromCall = exactFlowReturn();
let callCopy = fromCall;
callCopy = 2; // error[TK2322]

let fromMember: 1 | 2 = 2;
fromMember = flowHolder.value;
let memberCopy = fromMember;
memberCopy = 2; // error[TK2322]
