// Later loop assignments and repeated contextual signatures must not reuse provisional facts.

declare const keepLooping: boolean;
let loopState: 1 | 2 = 1;
while (keepLooping) {
  let beforeBackEdge = loopState;
  beforeBackEdge = 3; // error[TK2322]
  loopState = 2 as const;
}

declare function exactFirst(callback: (value: 1) => 1): 1;
declare function exactFirst(callback: (value: number) => number): number;
declare function broadFirst(callback: (value: number) => number): number;
declare function broadFirst(callback: (value: 1) => 1): 1;

const selectedExact = exactFirst((value) => value);
const selectedBroad = broadFirst((value) => value);
const exactSelection: 1 = selectedExact;
const broadSelection: 1 = selectedBroad; // error[TK2322]

const repeatedExact = exactFirst((value) => value);
const repeatedBroad = broadFirst((value) => value);
const repeatedExactSelection: 1 = repeatedExact;
const repeatedBroadSelection: 1 = repeatedBroad; // error[TK2322]
