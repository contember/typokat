class ConservativeSelf {
  tag = 1;
  member = { key: "alpha" } satisfies { // error[TK1360]
    key: ConservativeSelf extends { tag: number } ? "alpha" | "beta" : string;
  };
}
const conservativeSelf = new ConservativeSelf();
let conservativeSelfCopy = conservativeSelf.member.key;
conservativeSelfCopy = "alpha";
