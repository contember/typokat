// backlog 53: sequence expressions must be modeled by both the flow builder
// (inner assignments persist) and expression typing (value = last operand).
// Both marked lines are silent at the buggy HEAD.
function q1() {
  let x: string | null = null;
  x = null;
  (x = "s", 0);
  const y: null = x; // error[TK2322]: Type 'string' is not assignable to type 'null'
}
const n: number = (0, "b"); // error[TK2322]
