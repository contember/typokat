// A loop copy must see the closed back-edge union, not the larger declaration fallback.

declare const semanticLoopCondition: boolean;
let lateLocalState: 1 | 2 | 3 = 1;
lateLocalState = 1;
while (semanticLoopCondition) {
  let lateLocalCopy = lateLocalState;
  lateLocalCopy = 3; // error[TK2322]
  const laterLocalValue = 2 as const;
  lateLocalState = laterLocalValue;
}

declare function semanticLoopNext(): 2;
let callLoopState: 1 | 2 | 3 = 1;
callLoopState = 1;
while (semanticLoopCondition) {
  let callLoopCopy = callLoopState;
  callLoopCopy = 3; // error[TK2322]
  callLoopState = semanticLoopNext();
}

let cyclicLoopState: 1 | 2 | 3 = 1;
cyclicLoopState = 1;
while (semanticLoopCondition) {
  let cyclicLoopCopy = cyclicLoopState;
  cyclicLoopCopy = 3; // error[TK2322]
  cyclicLoopState = cyclicLoopState === 1 ? 2 : 1;
}

declare function checkedLoopNext(value: number): 2;
let checkedLoopState: 1 | 2 | 3 = 1;
checkedLoopState = 1;
while (semanticLoopCondition) {
  let checkedLoopCopy = checkedLoopState;
  checkedLoopCopy = 3; // error[TK2322]
  checkedLoopState = checkedLoopNext("wrong"); // error[TK2345]
}
