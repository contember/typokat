// backlog 53: assignments inside `&&`/`||` RHS and ternary arms must survive the
// expression — the post-expression state is a join of the branch-end cursors,
// not the pre-expression state. At the buggy HEAD every marked line is silent.
declare const cond: boolean;
function f_and() {
  let x: string | null = null;
  x = null;
  cond && (x = "s");
  const y: null = x; // error[TK2322]
  const z: string | null = x;
}
function f_or() {
  let x: string | null = null;
  x = null;
  cond || (x = "s");
  const y: null = x; // error[TK2322]
}
function f_tern() {
  let x: string | null = null;
  x = null;
  cond ? (x = "s") : 0;
  const y: null = x; // error[TK2322]
}
