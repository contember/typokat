// backlog 53: an assignment inside a `while` test must not be orphaned — the
// condition nodes antecede the post-test cursor (whose chain still reaches the
// loop label, preserving the fixpoint). At the buggy HEAD w1's marked line is
// silent and w2's body line is a false positive (x believed null).
declare const cond: boolean;
declare function next2(): string | null;
function w1() {
  let x: string | null = null;
  x = null;
  while ((x = "s") && cond) { }
  const y: null = x; // error[TK2322]: Type 'string' is not assignable to type 'null'
}
function w2() {
  let x: string | null = null;
  x = null;
  while (x = next2()) {
    const s: string = x;
  }
}
