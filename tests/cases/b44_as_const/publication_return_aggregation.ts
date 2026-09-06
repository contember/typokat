declare const aggregateFlag: boolean;

const aggregateExpression = (() => aggregateFlag ? true : 1) satisfies () => boolean | number;
const aggregateSingleBlock = (() => { return aggregateFlag ? true : 1; }) satisfies () => boolean | number;
const aggregateManyBlock = (() => { if (aggregateFlag) return true; return 1; }) satisfies () => boolean | number;
const aggregateExpressionExact: true | 1 = aggregateExpression();
const aggregateSingleExact: true | 1 = aggregateSingleBlock();
const aggregateManyExact: true | 1 = aggregateManyBlock();
let aggregateManyCopy = aggregateManyBlock();
aggregateManyCopy = false;
aggregateManyCopy = 2;

const aggregateEqual = (() => { if (aggregateFlag) return true; return true; }) satisfies () => boolean | number;
let aggregateEqualCopy = aggregateEqual();
aggregateEqualCopy = false; // error[TK2322]
const aggregateMixed = (() => { if (aggregateFlag) return true as const; return 1; }) satisfies () => boolean | number;
let aggregateMixedCopy = aggregateMixed();
aggregateMixedCopy = 2;
aggregateMixedCopy = false; // error[TK2322]
const aggregateMixedReversed = (() => { if (aggregateFlag) return 1; return true as const; }) satisfies () => number | boolean;
let aggregateMixedReversedCopy = aggregateMixedReversed();
aggregateMixedReversedCopy = 2;
aggregateMixedReversedCopy = false; // error[TK2322]

function aggregateOrdinaryFunction() {
  if (aggregateFlag) return true;
  return 1;
}
const aggregateOrdinaryExact: true | 1 = aggregateOrdinaryFunction();
let aggregateOrdinaryCopy = aggregateOrdinaryFunction();
aggregateOrdinaryCopy = false;
aggregateOrdinaryCopy = 2;
function aggregateExactFunction() {
  if (aggregateFlag) return 1;
  return 1 as const;
}
let aggregateExactCopy = aggregateExactFunction();
aggregateExactCopy = 2; // error[TK2322]

const aggregateAnnotated = (): boolean | number => aggregateFlag ? true : 1;
const aggregateAnnotatedExact: true | 1 = aggregateAnnotated(); // error[TK2322]
const aggregateExplicitVoid = (): void => true; // error[TK2322]
