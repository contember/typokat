// A static-member assignment normally infers its base before checking the write. When that base
// contains an arrow, function, or class expression, however, the deferred assignment LHS has no
// reserved lexical owners for the nested scope. Keep the skipped member-write semantics explicit,
// preserve independent assertion/class-expression owners, and infer the RHS only once.
//
// tsc 6.0.3 --strict: the first line reports TS2322, the second TS2540, and the invalid
// assertions report TS2352. The valid assertion, compound assignment, and wrapped controls are
// clean. Typokat conservatively owns every skipped base under backlog 71.

interface StaticMemberSource {
  source: string;
}

interface StaticMemberTarget {
  target: number;
}

declare const staticMemberSource: StaticMemberSource;

// No assertion: RHS assignability and readonly checking are both part of the skipped target.
((): { prop: number } => ({ prop: 0 }))().prop = "bad"; // incomplete[expr-infer/static-member-assignment/base]
(function(): { readonly prop: number } { return { prop: 0 }; })().prop = "bad"; // incomplete[expr-infer/static-member-assignment/base]

// Assertion compatibility remains an independent, additive owner. The valid assertion is still
// conservative because entering the function scope semantically would require unreserved owners.
(function(local: StaticMemberSource) { return { prop: <StaticMemberTarget>local }; })(staticMemberSource).prop = { target: 1 }; // incomplete[expr-infer/static-member-assignment/base] | incomplete[expr-infer/type-assertion/compatibility]
(function(local: StaticMemberSource) { return { prop: local as StaticMemberSource }; })(staticMemberSource).prop = staticMemberSource; // incomplete[expr-infer/static-member-assignment/base] | incomplete[expr-infer/as-assertion/compatibility]

// Compound writes and wrappers retain the same base owner even when tsc accepts the operation.
((): { prop: number } => ({ prop: 0 }))().prop += 1; // incomplete[expr-infer/static-member-assignment/base]
(((((): { prop: number } => ({ prop: 0 }))()) satisfies { prop: number })).prop = 1; // incomplete[expr-infer/static-member-assignment/base]

// A direct or wrapped class expression keeps its existing class-expression record in addition to
// the skipped static-member base. Assertions in the class body remain additive as well.
(class { static prop = 0; field = staticMemberSource as StaticMemberTarget; }).prop = 1; // incomplete[expr-infer/static-member-assignment/base] | incomplete[expr-infer/class-expression/self] | incomplete[expr-infer/as-assertion/compatibility]
((class { static prop = 0; } satisfies unknown)).prop = 1; // incomplete[expr-infer/static-member-assignment/base] | incomplete[expr-infer/class-expression/self]
