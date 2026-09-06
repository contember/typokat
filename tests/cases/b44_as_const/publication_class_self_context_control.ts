class ContextOwner {
  tag = 1;
  member = { key: "alpha" } satisfies {
    key: ContextOwner extends { tag: number } ? string : "alpha";
  };
}
let copy = new ContextOwner().member.key;
copy = "beta";
