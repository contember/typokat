// A static-member update normally infers its base through `infer_update_target`. Lexical
// reservation does not allocate owners for an arrow, function, or class nested in that update
// base, so entering the scope would panic or use the wrong checker state. Keep the skipped base
// explicit while preserving independent assertion and class-expression owners.
//
// tsc 6.0.3 --strict: only the two incompatible assertions report TS2352. The numeric prefix and
// postfix updates, the valid assertion, and the wrapper controls are otherwise clean.

interface StaticUpdateSource {
  source: string;
}

interface StaticUpdateTarget {
  target: number;
}

declare const staticUpdateSource: StaticUpdateSource;

// Function-expression base, postfix, no assertion.
(function() { // incomplete[expr-infer/static-member-update/base]
  return { prop: 0 };
})().prop++;

// Arrow base, prefix, with an incompatible angle assertion owned independently.
++((() => { // incomplete[expr-infer/static-member-update/base]
  const invalid = <StaticUpdateTarget>staticUpdateSource; // incomplete[expr-infer/type-assertion/compatibility]
  return { prop: 0 };
})()).prop;

// A valid assertion remains conservatively explicit because the function scope is not entered.
(function(local: StaticUpdateSource) { // incomplete[expr-infer/static-member-update/base]
  const valid = local as StaticUpdateSource; // incomplete[expr-infer/as-assertion/compatibility]
  return { prop: 0 };
})(staticUpdateSource).prop++;

// Direct class bases keep both the update-base and class-expression owners; a body assertion is
// additive rather than standing in for either surface.
(class { // incomplete[expr-infer/static-member-update/base] | incomplete[expr-infer/class-expression/self]
  static prop = 0;
  field = staticUpdateSource as StaticUpdateTarget; // incomplete[expr-infer/as-assertion/compatibility]
}).prop++;

// Parenthesized class/callable wrappers do not hide the nested lexical boundary.
++(((class { // incomplete[expr-infer/static-member-update/base] | incomplete[expr-infer/class-expression/self]
  static prop = 0;
}))).prop;

(((((function() { // incomplete[expr-infer/static-member-update/base]
  return { prop: 0 };
})())))).prop++;
