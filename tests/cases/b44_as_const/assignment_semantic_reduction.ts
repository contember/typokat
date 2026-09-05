// Reduction preserves declared surfaces and covers every assigned union contribution.

let broadAssignmentTarget: number = 0;
broadAssignmentTarget = 1 as const;
let broadAssignmentCopy = broadAssignmentTarget;
broadAssignmentCopy = 2;

let booleanAssignmentTarget: boolean = false;
booleanAssignmentTarget = true;
let ordinaryBooleanCopy = booleanAssignmentTarget;
ordinaryBooleanCopy = false;
booleanAssignmentTarget = true as const;
let exactBooleanCopy = booleanAssignmentTarget;
exactBooleanCopy = false; // error[TK2322]

let booleanUnionTarget: false | 1 = 1;
booleanUnionTarget = false;
let booleanUnionCopy = booleanUnionTarget;
booleanUnionCopy = true;

declare const partiallyInvalidSource: 1 | 3;
let partialCoverageTarget: 1 | 2 = 1;
partialCoverageTarget = partiallyInvalidSource; // error[TK2322]
let partialCoverageCopy = partialCoverageTarget;
partialCoverageCopy = 2;
partialCoverageCopy = 3; // error[TK2322]

const discriminatedReadonlySource = ({ kind: "a", value: 1 }) as const;
let mutableUnionTarget: { kind: "a"; value: number } | { kind: "b"; value: string } = { kind: "b", value: "start" };
mutableUnionTarget = discriminatedReadonlySource;
let mutableUnionCopy = mutableUnionTarget;
mutableUnionCopy.value = 2;
mutableUnionCopy.value = "wrong"; // error[TK2322]

let innerAssignmentTarget: 1 | 2 = 1;
let outerAssignmentTarget: 1 | 2 | 3 = 3;
let nestedAssignmentResult = (outerAssignmentTarget = (innerAssignmentTarget = 2));
nestedAssignmentResult = 4;
let nestedAssignmentCopy = outerAssignmentTarget;
nestedAssignmentCopy = 3; // error[TK2322]
