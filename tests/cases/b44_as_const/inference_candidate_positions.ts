// Candidate position and separate contribution preparation are independent of return shape.

class CandidatePositionHolder {
  readonly value = "read";
}
declare function projectCandidatePosition<T>(source: { value: T }): { value: T };
const projectedPosition = projectCandidatePosition(new CandidatePositionHolder());
const exactProjectedPosition: "read" = projectedPosition.value;
projectedPosition.value = "write"; // error[TK2322]
let mutableProjectedPosition = projectedPosition.value;
mutableProjectedPosition = "write";

declare function boxCandidatePair<T>(left: T, right: T): { value: T };
const equalCandidatePair = boxCandidatePair("a" as const, "a");
equalCandidatePair.value = "b";
const exactEqualCandidatePair: "a" = equalCandidatePair.value; // error[TK2322]
const reversedCandidatePair = boxCandidatePair("a", "a" as const);
reversedCandidatePair.value = "b";
const exactReversedCandidatePair: "a" = reversedCandidatePair.value; // error[TK2322]
const distinctCandidatePair = boxCandidatePair("a" as const, "b");
distinctCandidatePair.value = "c";
const regularCandidatePair = boxCandidatePair("a" as const, "b" as const);
regularCandidatePair.value = "c"; // error[TK2322]
boxCandidatePair("a", 1); // error[TK2345]

declare function intersectionReturnPosition<T>(value: T): T & {};
const intersectionPosition = intersectionReturnPosition("read");
const exactIntersectionPosition: "read" = intersectionPosition;
declare function selfConditionalReturnPosition<T>(value: T): T extends string ? T : never;
const selfConditionalPosition = selfConditionalReturnPosition("read");
const exactSelfConditionalPosition: "read" = selfConditionalPosition; // error[TK2322]
declare function independentConditionalReturnPosition<T, U>(value: T, condition: U): U extends true ? T : T | undefined;
const independentConditionalPosition = independentConditionalReturnPosition("read", true);
const exactIndependentConditionalPosition: "read" | undefined = independentConditionalPosition;
