// backlog 55 control: the same hash-consed template node reached through two
// different aliases, with no budget exhaustion anywhere, evaluates identically.
// Passes at HEAD; pins the healthy memo path around the fix.
type A1 = `x${Uppercase<"y">}`;
type A2 = `x${Uppercase<"y">}`;
const a1: A1 = "xY";
const a2: A2 = "xy"; // error[TK2322]: Type '"xy"' is not assignable to type '"xY"'
