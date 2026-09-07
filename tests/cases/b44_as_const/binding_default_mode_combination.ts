declare const combinationSource: { value?: "bar" };

let { value: combinationSame = "bar" } = combinationSource;
const combinationSameRead: "bar" = combinationSame;
combinationSame = "different"; // error[TK2322]

let { value: combinationDifferent = "foo" } = combinationSource;
combinationDifferent = "different";
const combinationDifferentWrong: "bar" | "foo" = combinationDifferent; // error[TK2322]

const { value: combinationFixed = "foo" } = combinationSource;
const combinationFixedRead: "bar" | "foo" = combinationFixed;
const combinationFixedWrong: "bar" = combinationFixed; // error[TK2322]

let { value: combinationAsserted = "foo" as const } = combinationSource;
combinationAsserted = "foo";
combinationAsserted = "different"; // error[TK2322]

declare const combinationUndefinedSource: { value: undefined };
let { value: combinationOnlyDefault = "bar" } = combinationUndefinedSource;
combinationOnlyDefault = "different";
combinationOnlyDefault = 1; // error[TK2322]

let { value: combinationAnnotated = "foo" }: { value?: "bar" } = combinationSource; // error[TK2322]
combinationAnnotated = "bar";
combinationAnnotated = "foo"; // error[TK2322]
