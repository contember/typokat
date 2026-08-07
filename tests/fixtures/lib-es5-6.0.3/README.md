# TypeScript 6.0.3 ES5 readiness fixture

This fixture proves the current type model against one exact `lib.es5.d.ts`; it does not load a
standard library into other source files. `readiness.toml` is the machine contract and records the
historical **GO for starting backlog 14** result. The namespace/declaration-merging lifecycle
closed that start gate; the full production default-library cutover subsequently shipped and was
archived after exact-`d1aa6d4` remote CI and final WU7 **PASS** with zero unresolved HIGH/MEDIUM
findings. This fixture remains an explicit-input model proof, not the
production-loader witness or checker 1.0 readiness. The owner-50 and owner-75 incompletes remain
release work; backlog `63` owns canonical Callable/Newable compatibility and surplus cardinality.

The authoritative artifact is the npm package output `lib/lib.es5.d.ts`, not the distinct upstream
source input `src/lib/es5.d.ts`. Verify a local TypeScript 6.0.3 installation against the pin with:

```sh
FIX=tests/fixtures/lib-es5-6.0.3
NPM_LIB="$(dirname "$(readlink -f "$(which tsc)")")/../lib/lib.es5.d.ts"
tsc --version
sha256sum "$NPM_LIB" "$FIX/lib.es5.d.ts"
wc -c -l "$NPM_LIB" "$FIX/lib.es5.d.ts"
cmp "$NPM_LIB" "$FIX/lib.es5.d.ts"
```

Run the strict TypeScript oracle over two explicit inputs. `--noLib` prevents an implicit second
copy of the standard library:

```sh
tsc --strict --noEmit --pretty false --noLib \
  "$FIX/lib.es5.d.ts" "$FIX/semantic-witnesses.ts"
```

The authoritative historical model check uses one synthetic source because it predates the shipped
shared standard-library storage and cross-file loading routes. The separator is one extra LF after the
byte-validated artifact; the artifact itself already ends in LF, so the synthetic source contains
one blank boundary line. A multi-file typokat invocation is non-authoritative loader evidence and
must not be used to judge these model witnesses.

```sh
COMBINED="$(mktemp --suffix=.ts)"
cp "$FIX/lib.es5.d.ts" "$COMBINED"
printf '\n' >> "$COMBINED"
sed -i '$r tests/fixtures/lib-es5-6.0.3/semantic-witnesses.ts' "$COMBINED"

tsc --strict --noEmit --pretty false --noLib "$COMBINED"
target/debug/typokat check --format compact "$FIX/lib.es5.d.ts"
target/debug/typokat check --format compact "$COMBINED"
rm "$COMBINED"
```

Both strict-tsc forms intentionally exit nonzero with exactly the 66 witness diagnostics recorded
in `readiness.toml`. At checker commit `23bad42`, the raw typokat artifact has four `TK2430`
diagnostics and 187 exact incompletes (179 owner 75 plus eight owner 50). The synthetic check keeps
that raw prefix and produces all 66 semantic witnesses as `TK2322`, including
`deep.Intl.value`, with no `TK2304` or added incomplete. The executable gate fingerprints the
complete ordered raw output, while the manifest fingerprints both source blobs; no residual is
hidden by an allowlist.
