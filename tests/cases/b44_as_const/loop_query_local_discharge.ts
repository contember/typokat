// TypeScript's incomplete-flow discharge is not a simultaneous expression fixed point.

declare const queryLocalLoopCondition: boolean;
declare function queryLocalStep<T extends 1 | 2 | 3>(value: T): T extends 1 ? 2 : T extends 2 ? 3 : 1;
let queryLocalLoopState: 1 | 2 | 3 | 4 = 1;
queryLocalLoopState = 1;
while (queryLocalLoopCondition) {
  let queryLocalLoopCopy = queryLocalLoopState;
  queryLocalLoopCopy = 3; // error[TK2322]
  queryLocalLoopState = queryLocalStep(queryLocalLoopState);
}

declare function checkedQueryLocalStep(value: 1): 2;
let checkedQueryLocalState: 1 | 2 | 3 = 1;
checkedQueryLocalState = 1;
while (queryLocalLoopCondition) {
  let checkedQueryLocalCopy = checkedQueryLocalState;
  checkedQueryLocalCopy = 3; // error[TK2322]
  checkedQueryLocalState = checkedQueryLocalStep(checkedQueryLocalState); // error[TK2345]
}
