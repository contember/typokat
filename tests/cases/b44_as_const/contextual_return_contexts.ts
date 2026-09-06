// Return context shapes nested expressions without replacing the inferred source return.

const nestedReturnContext: () => (value: number) => number = () => (value) => {
  value.toUpperCase(); // error[TK2339]
  return value;
};

const literalReturnSource = (() => "a") satisfies () => "a" | "b";
const exactLiteralReturnSource: "a" = literalReturnSource();
const objectReturnContext: () => { tag: "a" | "b" } = () => {
  return { tag: "a" };
};
const satisfiesVoidSource = (() => 1) satisfies () => void;
const actualVoidSourceReturn: number = satisfiesVoidSource();

const unannotatedBadDefault: (value?: number) => number = (value = "wrong") => value; // error[TK2322]
declare function unannotatedDefaultCall(value: number): number;
const unannotatedCalledDefault: (value?: number) => number = (value = unannotatedDefaultCall("wrong")) => value; // error[TK2345]
