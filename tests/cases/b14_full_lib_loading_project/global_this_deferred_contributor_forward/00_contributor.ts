// tsc 6.0.3 --strict: the script function contributes to globalThis, and its
// inferred return type requires checking the body in this source unit.
function B14DeferredGlobalThisContributor() {
  const value = 42;
  return value;
}
