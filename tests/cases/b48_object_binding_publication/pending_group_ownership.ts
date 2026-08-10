// Exact-storage pending-group review: a specific terminal use cannot consume a whole pattern,
// and a same-spelling use in another scope cannot own an outer pending leaf.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit` reports TS2339 on line 11,
// TS2322 on lines 12 and 19, and no diagnostic in `b48Shadowed`.

class B48PendingGroup {
  #value = 1;

  b48MixedUse(other: B48PendingGroup): void {
    const { ...copy } = other; // incomplete[bind/binding-pattern/object-pattern]
    copy.#value; // incomplete[expr-infer/private-field-access/self]
    const typed: number = copy; // error[TK2304]
    typed;
  }

  b48MultiLeaf(source: { owner: B48PendingGroup; sibling: string }): void {
    const { owner, ...sibling } = source; // incomplete[bind/binding-pattern/object-pattern]
    owner.#value; // incomplete[expr-infer/private-field-access/self]
    const typed: number = sibling; // error[TK2304]
    typed;
  }

  b48Shadowed(other: B48PendingGroup): void {
    const { ...sameName } = other; // incomplete[bind/binding-pattern/object-pattern]
    function nested(sameName: B48PendingGroup): void {
      sameName.#value; // incomplete[expr-infer/private-field-access/self]
    }
  }
}
