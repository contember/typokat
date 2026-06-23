// M3 — return statements checked against the declared return type.

function good(): number { return 1; }      // ok
function voidFn(): void { return; }        // ok — bare return under void
function inferRet(x: number) { return x; } // ok — return type inferred as number

function bad(): number { return "s"; }     // error[TK2322]: Type 'string' is not assignable to type 'number'

// DEFERRED (post-MVP, needs flow reachability): a non-void function with no return is NOT
// yet an error in typokat. tsc reports TK2355 here. Expect 0 errors for now.
function missingReturn(): number { }
