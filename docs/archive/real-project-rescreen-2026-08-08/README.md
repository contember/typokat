# Real-project re-screen evidence — 2026-08-08

This directory preserves the exact WU0 outputs behind the incomplete
[`real-project re-screen`](../sprint-2026-08-08-real-project-rescreen.md). None of the six immutable
candidates met backlog `72`'s unchanged zero threshold. The files were copied byte-for-byte from
the fresh screening roots before cleanup.

## Release and tools

- Release HEAD: `659e30ee97c53625d9a0412cd437e1224087147f` (`daaad0c` is an ancestor).
- Release binary: `target/release/typokat`, SHA-256
  `ac8ebc48de2136a6f7e15c9fcd56a8492ef24497c326002e63c6ddd2a90ce84c`.
- Oracle: `/run/user/1000/fnm_multishells/1002937_1784884227968/bin/tsc`, version `6.0.3`.
- Other tools: Git `2.54.0`, Node `24.4.0`, npm `11.19.0`, host pnpm `10.30.3`;
  `un-jinja` declares pnpm `11.1.3`.

The release build ran from the typokat repository root:

```sh
cpu-lease run -n 2 -- flock -w 3600 /tmp/typokat-perf.lock -c 'cargo build --release'
```

Each candidate used a distinct empty `/tmp/typokat-b72-<slug>-XXXXXX` root. Checkout used
`git -c protocol.file.allow=never clone --no-checkout <canonical-https-remote> <root>/repo`, then a
detached checkout of the pinned commit. No Git object cache was shared. The check command shapes
were:

```sh
(cd "$ROOT/repo" && "$TSC" --pretty false --strict --noEmit -p tsconfig.json) \
  >"$ROOT/tsc-native.stdout" 2>"$ROOT/tsc-native.stderr"
"$TSC" --pretty false -p "$ROOT/tsconfig.json" \
  >"$ROOT/tsc-overlay.stdout" 2>"$ROOT/tsc-overlay.stderr"
"$BIN" check --project-summary json "$ROOT/tsconfig.json" \
  >"$ROOT/typokat.stdout" 2>"$ROOT/typokat.stderr"
```

No install ran for the first four candidates. `un-jinja` ran
`cpu-lease run -n 2 -- corepack pnpm@11.1.3 install --frozen-lockfile --ignore-scripts` from its
repository. `south-african-id` ran
`cpu-lease run -n 2 -- pnpm install --frozen-lockfile --ignore-scripts`.

## Candidate identities and results

License and lockfile digests were verified before execution. `typokat` links point to the exact
stdout JSON. The JSON preserves exact roots, resolution graph and module forms, checked/skipped
files, notices, parse errors, incomplete records, and diagnostics.

| Candidate | Canonical remote and commit | License path / SHA-256 | Lock path / SHA-256 | Exits native / overlay / typokat | Exact output |
| --- | --- | --- | --- | --- | --- |
| `morkg/jabr` | `https://github.com/morkg/jabr.git` at `9415fdad8b98dc0f1aba09c8badc5fc209bc30ba` | `LICENSE` / `1256366f990b3fa2b0780d082cae641a126c50cd5fdbe77acff3b45acfe056c2` | `package-lock.json` / `1825f799ba12dee085a2da8ef33768efe9fe98bb0827ab9be7af384cf87070a5` | `0 / 0 / 3` | [`jabr.json`](jabr.json), SHA-256 `eaba8c19750c6346d32c2c65adbdc899d75f2b83732aa41f884c7b5c0af39daf` |
| `lokicik/placetext` | `https://github.com/lokicik/placetext.git` at `faf233107146ceca63bf8a6fec8f07ad43ab17e2` | `LICENSE` / `52578f8c669574581e8a046ee80ec13827c006dc173af8f28621449516a52633` | `package-lock.json` / `6632ffc7fce92584a119fbce40647358703915a93f4ba382a8c90e61278642b0` | `0 / 0 / 3` | [`placetext.json`](placetext.json), SHA-256 `fb2478ac449e4b5bf074d38515efa7f9999e958b3ad9771158cb0dd97616840c` |
| `naoeosavio/lite-fp` | `https://github.com/naoeosavio/lite-fp.git` at `09865973c3599928df272fc6f79c9daf9a955bc5` | `LICENSE` / `4bc6a360f7bab8b5c4b175bc24751c931f27bc5b4196a4cf8709fa9f624514d9` | `package-lock.json` / `1a871ec1ddd676215fa2e125940c7d3cc9fc503fe03f8f68b475174c98d9e4d9` | `2 / 0 / 3` | [`lite-fp.json`](lite-fp.json), SHA-256 `86f15a914fe7d85683ce1b437a62c01c015cb3534f2f4681c81fca309552ee6a`; exact native oracle stdout [`lite-fp-tsc-native.txt`](lite-fp-tsc-native.txt), SHA-256 `f3fe1f798055e146eccec5c0fad0a8e3f97eb1309602959b37801af29c06a8b6` |
| `jacob-bennett/deco` | `https://github.com/jacob-bennett/deco.git` at `daa5feaa886de0727807aa12ea6ff2f4d7841f60` | `LICENSE.txt` / `7530c8d9c1f25c7b5b85bca3b75db0165c0f2893d2736b5ffa615ce1786bd290` | `package-lock.json` / `09ec03451feff0df14edf3deab3b8929fbd37e97745d09aeea5dec0361112616` | `0 / 2 / 3` | [`deco.json`](deco.json), SHA-256 `3fa932a318e6564d1339c1dc28b1554f975d50c9904cd4b8b7ff2e8f69fad08c`; exact overlay oracle stdout [`deco-tsc-overlay.txt`](deco-tsc-overlay.txt), SHA-256 `9ec5b43c325e9d7e6ae6a5a183b874f9744276ade10626291711597fe69b56a3` |
| `theetherGit/un-jinja` | `https://github.com/theetherGit/un-jinja.git` at `d43537ec4611e694528899dbfb97cbdc4b24b86c` | `LICENSE.md` / `3822d9bb8c5f39a4a07939371ff72adbbba20fade6c202523f21cfc7f3ef01b7` | `pnpm-lock.yaml` / `b9bdf32d064348ff28df2a23121bb6e7d0fae9d3b85df596fde8cec5322d4559` | `0 / 0 / 3` | [`un-jinja.json`](un-jinja.json), SHA-256 `61444201e912a3f3427025263f34f80d225ed90635c8461367d2aaca52588be2` |
| `SiphoChris/south-african-id` | `https://github.com/SiphoChris/south-african-id.git` at `4e8ab8ac4e6bd8109983a7db6adbf39a3c422a61` | `LICENSE` / `eaa832a918a94cc080c2d2edf5b7b83b64a44a74797e1e99635bb0f8d2c5b727` | `pnpm-lock.yaml` / `3c2469deb9494cfee4cd8fefc4a74b920de81ba6b4ff85b4334d49e44805cbdf` | `0 / 0 / 1` | [`south-african-id.json`](south-african-id.json), SHA-256 `f035270629e792efbebff0f39adb73282c419b078cccfa370575e87392bf1662` |

Every unlinked oracle stdout/stderr file was empty, SHA-256
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`. The exact nonempty oracle
outputs are retained above. The archive sprint records the corresponding checked/skipped counts
and first blockers.

## Ambient inventory and qualification limits

The selected production subtrees contain no Node or Bun ambient references. `lite-fp` alone adds
`declare global` augmentations for `Promise<T>` and `Array<T>`. The native `un-jinja` and
`south-african-id` programs consume direct `@types/node`; their transparent overlays exclude
tooling/tests, so neither overlay is the exact native program and neither candidate can qualify on
this evidence. The overlays also drop native compiler options. In particular, `placetext`'s
target/library equivalence is plausible but unproved, and its default-export rejection preempts
semantic checking.

An independent reviewer reproduced all six exit triples, verified clean checkouts and every output
channel, and returned **PASS** on the WU0 hard stop.
