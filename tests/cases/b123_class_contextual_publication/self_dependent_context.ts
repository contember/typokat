class NarrowContextOwner {
  tag = 1;
  member = { key: "alpha" } satisfies {
    key: NarrowContextOwner extends { tag: number } ? "alpha" | "beta" : string;
  };
}
let narrowCopy = new NarrowContextOwner().member.key;
narrowCopy = "beta"; // error[TK2322]
