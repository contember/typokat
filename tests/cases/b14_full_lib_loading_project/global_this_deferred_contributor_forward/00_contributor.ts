// The explicit return keeps this fixture scoped to globalThis publication.
function B14DeferredGlobalThisContributor(): number {
  const value = 42;
  return value;
}
