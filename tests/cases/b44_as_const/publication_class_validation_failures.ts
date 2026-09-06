interface FailureKeys { alpha: number; beta: string }
interface FailureHolder { key: keyof FailureKeys }
declare const failureFlag: boolean;
class SuccessfulThenUnsupported {
  good = { key: "alpha" } satisfies FailureHolder; // error[TK1360]
  defaults = ((value: number = "bad") => value) satisfies (value?: number) => number; // error[TK2322] | incomplete[class/property-definition/initializer-inference]
  missing = { key: missingSource } satisfies FailureHolder; // error[TK2304] | incomplete[class/property-definition/initializer-inference]
  conditional = failureFlag ? missingBranch : 1; // error[TK2304] | incomplete[class/property-definition/initializer-inference]
  annotation = { key: "alpha" } satisfies MissingTarget; // error[TK2304] | incomplete[class/property-definition/initializer-inference]
}
class UnsupportedThenSuccessful {
  defaults = ((value: number = "bad") => value) satisfies (value?: number) => number; // error[TK2322] | incomplete[class/property-definition/initializer-inference]
  missing = { key: missingOtherSource } satisfies FailureHolder; // error[TK2304] | incomplete[class/property-definition/initializer-inference]
  conditional = failureFlag ? missingOtherBranch : 1; // error[TK2304] | incomplete[class/property-definition/initializer-inference]
  annotation = { key: "alpha" } satisfies MissingOtherTarget; // error[TK2304] | incomplete[class/property-definition/initializer-inference]
  good = { key: "alpha" } satisfies FailureHolder; // error[TK1360]
}
