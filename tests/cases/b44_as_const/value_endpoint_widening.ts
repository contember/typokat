// Assignment and sequence expressions follow their value-producing endpoint when deciding
// whether to widen a literal result.

let ordinarySink = 0;
let ordinaryAssignmentResult = (ordinarySink = 1);
ordinaryAssignmentResult = 2;

declare function sideEffect(): void;
let ordinarySequenceResult = (sideEffect(), 1);
ordinarySequenceResult = 2;

let constSequenceResult = (sideEffect(), 1 as const);
constSequenceResult = 2; // error[TK2322]: Type '2' is not assignable to type '1'

let constTarget: 1 | 2 = 2;
let constAssignmentResult = (constTarget = 1 as const);
constAssignmentResult = 2; // error[TK2322]: Type '2' is not assignable to type '1'
