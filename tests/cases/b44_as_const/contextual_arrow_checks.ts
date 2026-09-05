// Contextual arrows keep parameter effects and infer their actual return type.

const invalidAnnotatedDefault: (value?: number) => number = (value: number = "wrong") => value; // error[TK2322]
declare function checkedDefaultValue(value: number): number;
const invalidCalledDefault: (value?: number) => number = (value: number = checkedDefaultValue("wrong")) => value; // error[TK2345]
const validAnnotatedDefault: (value?: number) => number = (value: number = 1) => value;

const emptyNumberCallback: () => number = () => {}; // error[TK2322]
const bareNumberCallback: () => number = () => { return; }; // error[TK2322]
const expressionVoidCallback: () => void = () => 1;
const blockVoidCallback: () => void = () => { return 1; };
const explicitVoidReturn = (): void => 1; // error[TK2322]

const exactContextualCallback: (value: 1) => number = (value) => {
  let exactContextualCopy = value;
  exactContextualCopy = 2; // error[TK2322]
  return value;
};
