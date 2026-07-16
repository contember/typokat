# TypeScript 6.0.3 ES5 readiness fixture

This fixture proves the current type model against one exact `lib.es5.d.ts`; it does not load a
standard library into other source files. `readiness.toml` is the machine contract and records the
current NO-GO result.

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

The authoritative typokat model check uses one synthetic source because shared standard-library
storage and cross-file loading belong to backlog 14. The separator is one extra LF after the
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
in `readiness.toml`. The raw typokat artifact and synthetic-source results are also pinned there.
