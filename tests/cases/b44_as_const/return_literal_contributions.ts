// Return inference combines literal contributions before applying its widening policy.

declare const returnContributionCondition: boolean;
function ordinaryThenExactReturn() {
  if (returnContributionCondition) return 1;
  return 1 as const;
}
let ordinaryThenExactCopy = ordinaryThenExactReturn();
ordinaryThenExactCopy = 2; // error[TK2322]

function exactThenOrdinaryReturn() {
  if (returnContributionCondition) return 1 as const;
  return 1;
}
let exactThenOrdinaryCopy = exactThenOrdinaryReturn();
exactThenOrdinaryCopy = 2; // error[TK2322]

function differentReturnContributions() {
  if (returnContributionCondition) return 1;
  return 2 as const;
}
let differentReturnCopy = differentReturnContributions();
differentReturnCopy = 3;
