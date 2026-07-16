// Surface-accounting spec (shipped namespace path / backlog 75). Qualified type-group lowering is
// supported; `this` types and type predicates
// remain recorded before degrading. See tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `lower_annotation_inner` records `TSThisType` / `TSTypePredicate` before
// withholding those annotations.

namespace A {
  export interface B {}
}

// WU3: the public type-group leaf resolves through the immutable registry.
type Q = A.B;

// INCOMPLETE: a type predicate return annotation is not lowered (owner 50).
function isStr(x: unknown): x is string { // incomplete[annotation-lower/type-predicate/self]
  return typeof x === "string";
}

class Chain {
  // INCOMPLETE: a polymorphic `this` return type is not modeled (owner 75).
  self(): this { // incomplete[annotation-lower/this-type/self]
    return this;
  }
}
