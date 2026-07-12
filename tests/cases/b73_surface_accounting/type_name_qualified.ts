// Surface-accounting spec (backlog 43 / 70). ENABLED by WU5: annotation lowering records
// the incomplete surface for qualified type names, `this` types, and type predicates
// before degrading. See tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `resolve_type_reference` returned `None` for a qualified/`this` name,
// and `lower_annotation_inner` dropped `TSThisType` / `TSTypePredicate` silently.

// INCOMPLETE: a qualified type name `A.B` is not resolved (owner 43).
type Q = A.B; // incomplete[annotation-lower/type-name/qualified-name]

// INCOMPLETE: a type predicate return annotation is not lowered (owner 50).
function isStr(x: unknown): x is string { // incomplete[annotation-lower/type-predicate/self]
  return typeof x === "string";
}

class Chain {
  // INCOMPLETE: a `this` return type is not modeled (owner 70).
  self(): this { // incomplete[annotation-lower/this-type/self]
    return this;
  }
}
