// Backlog 87 — the cap is a property of the chain renderer, not of one reason variant,
// so the array descent (`element_reason_lines`) is pinned on both sides of the boundary
// too. `string[]` nested 16 deep produces exactly 16 elaboration levels: 15 array levels
// plus the leaf mismatch. Nothing is omitted, and the level-15 `string[]`/`number[]` line
// is the discriminator — an early cap would swallow it into the elision.

declare const deepArray: string[][][][][][][][][][][][][][][][];
const atCap: number[][][][][][][][][][][][][][][][] = deepArray; // error[TK2322]: Type 'string[]' is not assignable to type 'number[]'.
