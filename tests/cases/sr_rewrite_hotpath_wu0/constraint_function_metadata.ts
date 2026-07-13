// tsc 6.0.3 --strict: only take(bad) reports TS2345 and retains the "ok" constraint.

type EvaluatedConstraint =
    <U extends "ok" = "ok">(
        this: Uppercase<"r">,
        value?: Uppercase<"p">,
        ...tail: U[]
    ) => U;

function takeEvaluated<T extends EvaluatedConstraint>(fn: T): void {}

declare const goodEvaluated:
    <U extends "ok" = "ok">(this: "R", value?: "P", ...tail: U[]) => U;
declare const badEvaluated:
    <U extends "bad" = "bad">(this: "R", value?: "P", ...tail: U[]) => U;

takeEvaluated(goodEvaluated);
takeEvaluated(badEvaluated); // error[TK2345]: "ok"
