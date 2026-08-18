// WU3b acceptance: same-scope fragments merge; sibling block groups remain distinct.

interface Merged {
  first: string;
}

interface Merged {
  second: number;
}

const missingMerged: Merged = { first: "ready" }; // error[TK2741]

{
  interface Shadowed {
    left: string;
  }
  const invalidLeft: Shadowed = { left: 1 }; // error[TK2322]
}

{
  interface Shadowed {
    right: number;
  }
  const invalidRight: Shadowed = { right: "wrong" }; // error[TK2322]
}
