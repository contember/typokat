// M3 — function-type assignability: parameters CONTRAVARIANT, return COVARIANT (§6.5).
// (typokat is contravariant everywhere; this matches tsc under strictFunctionTypes for
//  function-typed values. The bivariance deviation only shows up for METHODS, which are
//  not in the MVP subset.) Targets are function types -> code-only markers.

let fn: (x: number) => number = (x: number) => x;
fn = (x: unknown) => 1; // ok — param widened: number <: unknown (contravariant)
fn = (x: string) => 1;  // error[TK2322]

let producer: () => number = () => 1;
producer = () => 2;   // ok
producer = () => "s"; // error[TK2322]
