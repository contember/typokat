// M31 — a source intersection assigned to a RECURSIVE object target must TERMINATE:
// a single contributor to a target property delegates to the cycle-guarded relation
// engine; a multi-contributor recursive property uses the assume-true cycle guard.
// Regression net for the finding-1 fix, whose merged per-property walk must not
// recurse unguarded (stack overflow).

interface Rec { next: Rec; }
declare const r: Rec & { extra: number };
const okRec: Rec = r;                                  // clean — merged {next:Rec; extra} satisfies Rec

interface Cell { next: Cell; value: number; }
declare const cell: Cell & { extra: string };
const badCell: { next: Cell; value: string } = cell;   // error[TK2322]: is not assignable to type 'string'

// multi-contributor recursive: both members contribute the recursive key `p`
interface P { p: P; }
interface Q { p: Q; }
declare const pq: P & Q;
const okPQ: P = pq;                                    // clean — coinductive fixpoint
