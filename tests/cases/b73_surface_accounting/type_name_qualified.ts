// Surface-accounting spec (backlog 43 / 75). WU2 classifies the successful qualified
// path but leaves its type-group lowering to WU3; `this` types and type predicates
// remain recorded before degrading. See tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `resolve_type_reference` returns `None` after classifying the qualified
// type group (or for a `this` name), and `lower_annotation_inner` drops `TSThisType` /
// `TSTypePredicate` silently.

namespace A { // incomplete[decl/module-declaration/self]
  export interface B {}
}

// INCOMPLETE: the public type-group leaf is classified but not lowered until WU3 (owner 43).
type Q = A.B; // incomplete[annotation-lower/type-name/qualified-name]

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
