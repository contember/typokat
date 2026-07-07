// backlog 53 regression net: the loop-label fixpoint discipline (invariants §1 —
// never memoize the provisional seed) already produces both errors at HEAD and
// must survive the cursor-join rework unchanged.
declare const cond: boolean;
declare function step(): string | null;
function r1() {
  let x: string | null = "a";
  if (x === null) return;
  while (cond) {
    const s: string = x; // error[TK2322]
    x = step();
  }
  const t: string = x; // error[TK2322]
}
