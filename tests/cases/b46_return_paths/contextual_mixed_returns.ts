declare const contextualReturnCondition: boolean;

const mixedBareReturn: () => number = () => { // error[TK2322]
  if (contextualReturnCondition) return 1;
  return;
};
const partialReturn: () => number = () => { // error[TK2322]
  if (contextualReturnCondition) return 1;
};
