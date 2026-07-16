// Deferred assignment targets do not semantically enter nested lexical scopes. Assertions inside
// those scopes must still own the existing compatibility incomplete instead of being false-clean
// or being inferred with the assignment's outer ScopeId.
//
// tsc 6.0.3 --strict: the three incompatible assertions report TS2352. The compatible controls
// are clean at the assertion site; typokat conservatively records the same unsupported
// compatibility surface because the enclosing assignment target remains deferred.

interface DeferredAssertionSource {
  source: string;
}

interface DeferredAssertionTarget {
  target: number;
}

declare const deferredSource: DeferredAssertionSource;
declare let deferredKeyed: { [key: string]: number };
declare let deferredSink: unknown;

// Incompatible assertions in an arrow body, a function body used as a destructuring default,
// and a class-expression field all retain their assertion-specific owner.
deferredKeyed[String(<T extends DeferredAssertionSource>(local: T) =>
  local as DeferredAssertionTarget // incomplete[expr-infer/as-assertion/compatibility]
)] = 1;

[deferredSink = function(local: DeferredAssertionSource) {
  return <DeferredAssertionTarget>local; // incomplete[expr-infer/type-assertion/compatibility]
}] = [1];

deferredKeyed[String(class { // incomplete[expr-infer/class-expression/self]
  field = deferredSource as DeferredAssertionTarget; // incomplete[expr-infer/as-assertion/compatibility]
})] = 1;

// Conservative controls: compatibility is not inferred through the nested scope even when the
// source and asserted types are identical.
deferredKeyed[String((local: DeferredAssertionSource) =>
  local as DeferredAssertionSource // incomplete[expr-infer/as-assertion/compatibility]
)] = 1;

[deferredSink = function(local: DeferredAssertionSource) {
  return <DeferredAssertionSource>local; // incomplete[expr-infer/type-assertion/compatibility]
}] = [1];

deferredKeyed[String(class { // incomplete[expr-infer/class-expression/self]
  field = deferredSource as DeferredAssertionSource; // incomplete[expr-infer/as-assertion/compatibility]
})] = 1;
