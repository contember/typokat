// Update-expression reservation does not allocate lexical owners for an arrow, function, or class
// nested anywhere in its SimpleAssignmentTarget. Entering that scope would panic or use the wrong
// checker state, so every representable target family keeps one target-wide owner while assertion
// compatibility and class-expression records remain independent and additive.
//
// tsc 6.0.3 --strict: the four incompatible inner assertions report TS2352. Static, computed,
// private-field, and assertion/`satisfies`/non-null wrapper targets are otherwise clean.

interface NestedUpdateSource {
  source: string;
}

interface NestedUpdateTarget {
  target: number;
}

declare const nestedUpdateSource: NestedUpdateSource;
declare let nestedUpdateValues: { [key: string]: number };

// Static-member bases: postfix function/no assertion and prefix arrow/no assertion.
(function() { // incomplete[expr-infer/update-expression/nested-scope-target]
  return { prop: 0 };
})().prop++;

++((() => ({ prop: 0 }))()).prop; // incomplete[expr-infer/update-expression/nested-scope-target]

// Computed-member object and key positions each retain the same target-wide identity.
++((function() { // incomplete[expr-infer/update-expression/nested-scope-target]
  return nestedUpdateValues;
})()["slot"]);

nestedUpdateValues[(() => { // incomplete[expr-infer/update-expression/nested-scope-target]
  const valid = nestedUpdateSource as NestedUpdateSource; // incomplete[expr-infer/as-assertion/compatibility]
  return "slot";
})()]++;

// A class in the computed key preserves its class-expression owner and a valid body assertion.
nestedUpdateValues[class { // incomplete[expr-infer/update-expression/nested-scope-target] | incomplete[expr-infer/class-expression/self]
  static key = "slot";
  field = nestedUpdateSource as NestedUpdateSource; // incomplete[expr-infer/as-assertion/compatibility]
}.key]++;

// Every representable assertion/wrapper target is guarded at the target root. Each wrapper keeps
// a nested callable through its underlying static member, while compatibility records remain
// conservative and additive.
(((function(local: NestedUpdateSource) { const invalid = local as NestedUpdateTarget; return { prop: 0 }; })(nestedUpdateSource).prop) as number)++; // incomplete[expr-infer/update-expression/nested-scope-target] | incomplete[expr-infer/as-assertion/compatibility] | incomplete[expr-infer/as-assertion/compatibility]
++(<number>((function(local: NestedUpdateSource) { const invalid = <NestedUpdateTarget>local; return { prop: 0 }; })(nestedUpdateSource).prop)); // incomplete[expr-infer/update-expression/nested-scope-target] | incomplete[expr-infer/type-assertion/compatibility] | incomplete[expr-infer/type-assertion/compatibility]
(((function(local: NestedUpdateSource) { const invalid = local as NestedUpdateTarget; return { prop: 0 }; })(nestedUpdateSource).prop) satisfies number)++; // incomplete[expr-infer/update-expression/nested-scope-target] | incomplete[expr-infer/as-assertion/compatibility]
(((function(local: NestedUpdateSource) { const valid = local as NestedUpdateSource; return { prop: 0 }; })(nestedUpdateSource).prop)!)++; // incomplete[expr-infer/update-expression/nested-scope-target] | incomplete[expr-infer/as-assertion/compatibility]

// Private-field objects are representable inside the declaring class and use the same owner.
class NestedUpdatePrivateHolder {
  #value = 0;

  update() {
    (function(owner: NestedUpdatePrivateHolder) { // incomplete[expr-infer/update-expression/nested-scope-target]
      return owner;
    })(this).#value++;

    ++((() => this)()).#value; // incomplete[expr-infer/update-expression/nested-scope-target]
  }
}

// Nested wrappers around a class-valued static target keep the target, class, and assertion owners.
((((class { // incomplete[expr-infer/update-expression/nested-scope-target] | incomplete[expr-infer/class-expression/self] | incomplete[expr-infer/as-assertion/compatibility]
  static prop = 0;
  field = nestedUpdateSource as NestedUpdateTarget; // incomplete[expr-infer/as-assertion/compatibility]
})!) satisfies unknown) as { prop: number }).prop++;
