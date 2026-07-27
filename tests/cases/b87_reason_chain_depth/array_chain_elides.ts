// Backlog 87 — the same array descent one step past the cap. `string[]` nested 20 deep
// produces 20 elaboration levels, so levels 1-16 render and the remaining three array
// levels collapse into one elision line followed by the retained innermost cause.

declare const deepArray: string[][][][][][][][][][][][][][][][][][][][];
const elides: number[][][][][][][][][][][][][][][][][][][][] = deepArray; // error[TK2322]: ... 3 more nested levels omitted.
const retainsLeaf: number[][][][][][][][][][][][][][][][][][][][] = deepArray; // error[TK2322]: Type 'string' is not assignable to type 'number'.
