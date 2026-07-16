// Assignment-expression LHS reservation does not allocate lexical owners for an arrow, function,
// or class nested anywhere in its AssignmentTarget. Every representable target family therefore
// keeps one target-wide owner; assertion compatibility and class-expression records are additive,
// the nested body is not entered, and the RHS is still inferred exactly once.
//
// tsc 6.0.3 --strict: TS2304 on both intentionally missing names and TS2352 on the incompatible
// class-body assertion. All other target-family controls are clean.

interface NestedAssignmentSource {
  source: string;
}

interface NestedAssignmentTarget {
  target: number;
}

declare const nestedAssignmentSource: NestedAssignmentSource;
declare let nestedAssignmentValues: { [key: string]: number };
declare let nestedAssignmentSlot: number;
declare let nestedAssignmentObject: { slot: number; rest: number[] };
declare let nestedAssignmentRest: { [key: string]: unknown };

// Static target: the body error stays suppressed while the RHS diagnostic fires exactly once.
(function() { MissingAssignmentBody; return { prop: 0 }; })().prop = MissingAssignmentRhs; // error[TK2304]: Cannot find name 'MissingAssignmentRhs' | incomplete[expr-infer/assignment-expression/nested-scope-target]

// Computed-member object and key positions share the target-wide identity.
(function() { return nestedAssignmentValues; })()["slot"] = 1; // incomplete[expr-infer/assignment-expression/nested-scope-target]
nestedAssignmentValues[(() => "slot")()] = 1; // incomplete[expr-infer/assignment-expression/nested-scope-target]

// Assertion, angle, satisfies, and non-null wrappers are guarded before semantic assertion walk.
(((function() { return { prop: 0 }; })().prop) as number) = 1; // incomplete[expr-infer/assignment-expression/nested-scope-target] | incomplete[expr-infer/as-assertion/compatibility]
(<number>((function() { return { prop: 0 }; })().prop)) = 1; // incomplete[expr-infer/assignment-expression/nested-scope-target] | incomplete[expr-infer/type-assertion/compatibility]
(((function() { return { prop: 0 }; })().prop) satisfies number) = 1; // incomplete[expr-infer/assignment-expression/nested-scope-target]
(((function() { return { prop: 0 }; })().prop)!) = 1; // incomplete[expr-infer/assignment-expression/nested-scope-target]

// A class-valued static target retains class and assertion owners independently.
(class { static prop = 0; field = nestedAssignmentSource as NestedAssignmentTarget; }).prop = 1; // incomplete[expr-infer/assignment-expression/nested-scope-target] | incomplete[expr-infer/class-expression/self] | incomplete[expr-infer/as-assertion/compatibility]

// Array destructuring: element target, default initializer, and rest target.
[(function() { return nestedAssignmentObject; })().slot] = [1]; // incomplete[expr-infer/assignment-expression/nested-scope-target]
[nestedAssignmentSlot = (() => 0)()] = [1]; // incomplete[expr-infer/assignment-expression/nested-scope-target]
[...(function() { return nestedAssignmentObject; })().rest] = []; // incomplete[expr-infer/assignment-expression/nested-scope-target]

// Object destructuring: computed key, property target, default initializer, and rest target.
({ [((): "slot" => "slot")()]: nestedAssignmentSlot } = { slot: 1 }); // incomplete[expr-infer/assignment-expression/nested-scope-target]
({ slot: (function() { return nestedAssignmentObject; })().slot } = { slot: 1 }); // incomplete[expr-infer/assignment-expression/nested-scope-target]
({ slot: nestedAssignmentSlot = (() => 0)() } = {}); // incomplete[expr-infer/assignment-expression/nested-scope-target]
({ ...(function() { return { rest: nestedAssignmentRest }; })().rest } = {}); // incomplete[expr-infer/assignment-expression/nested-scope-target]

// Private-field objects are representable inside the declaring class.
class NestedAssignmentPrivateHolder {
  #value = 0;

  assign() {
    (function(owner: NestedAssignmentPrivateHolder) { return owner; })(this).#value = 1; // incomplete[expr-infer/assignment-expression/nested-scope-target]
  }
}
