# Bundler project tracer corpus

Disabled WU1 acceptance corpus for backlogs 15 and 72. Each child directory is one project. Run the
oracle from this directory with the pinned TypeScript 6.0.3 binary:

```sh
for project in */tsconfig.json; do
  tsc --pretty false --noEmit -p "$project"
done
```

`contract.json` records exact oracle and future typokat identities. The raw conformance row remains
disabled because that harness does not read configs or summaries. WU3 instead unignores
`tests/b72_bundler_project_tracer_cli.rs`. The public typokat invocation is either the project
directory or its `tsconfig.json`:

```sh
typokat check --project-summary json <project-directory>
typokat check --project-summary json <project-directory>/tsconfig.json
```

The two forms must emit the same single-line JSON summary on stdout. Existing diagnostics remain on
stderr. Unsupported project input exits 3 and does not enter semantic checking; usage/config IO
failures exit 2, ordinary diagnostics exit 1, and a complete clean run exits 0. Source parse
rejection precedes module inventory because a recovered AST is not authoritative. Incomplete
surfaces take exit 3 over ordinary diagnostics, while both identities remain in the summary.

Only `strict: true`, `noEmit: true`, `module: "ESNext"`, `moduleResolution: "Bundler"`, and a
non-empty project-relative `files` array of `.ts` roots are admitted. Missing or wrong required
compiler options, `paths`, `baseUrl`, `lib` (the generic unconsumed-option control), missing, empty,
non-array, or non-string `files`; missing/outside/absolute/unsupported roots; `include`, `exclude`,
`extends`, `references`; and unconsumed compiler options are exact non-clean config identities.
Every import/export declaration is inventoried before semantic filtering. Default, namespace,
side-effect, bare, star, namespace-re-export, source-re-export, import-equals, export-assignment,
default-export, and namespace-export forms fail closed. A declaration with a default binding
collapses to one `default-import` identity. Resolution identities include declaration columns; a
same-line two-import control prevents accidental deduplication. The admitted root list also names
one file as both `./value.ts` and `value.ts`; both normalize to one sorted root.

Pinned WU1 oracle and negative-control results:

| Case | `tsc 6.0.3 -p` | pre-change production CLI |
|---|---|---|
| admitted `.js` → `.ts` | `TS2322`, exit 2 | wrong `TK2307`, exit 1 |
| admitted extensionless | `TS2322`, exit 2 | supported semantic control |
| missing local | `TS2307`, exit 2 | `TK2307`, exit 1 |
| absent bare import, no use | `TS2307`, exit 2 | **false-clean**, exit 0 |
| default import, no use | clean, exit 0 | **false-clean**, exit 0 |
| namespace import, no use | clean, exit 0 | **false-clean**, exit 0 |
| side-effect import | clean, exit 0 | **false-clean**, exit 0 |
| source re-export, no consumer | clean, exit 0 | **false-clean**, exit 0 |
| star / namespace re-export | clean, exit 0 | **false-clean**, exit 0 |
| import-equals / export-assignment / namespace export | exact `tsc` result in `contract.json` | unsupported or incomplete |
| local cycle | clean, exit 0 | wrong resolution/publication behavior |
| NodeNext config | clean, exit 0 | config unseen; source alone exits clean |
| source parse error | `TS1109`, exit 2 | parser error, exit 1 |
| array spread incomplete | clean, exit 0 | exact incomplete identity, exit 3 |

The existing explicit-file route is also guarded. Its admitted extensionless diagnostic stays
byte-identical. Its current `.js` import result deliberately stays `TK2307`; `.js` → `.ts`
substitution belongs only to config-backed project mode in this tracer. Every previously filtered
module form becomes exit 3 after WU3.

The public directory input currently exits 2 with `Is a directory`; `tsconfig.json` is parsed as
TypeScript source and exits 1. Force-run the ignored black-box RED contract with:

```sh
cargo test --test b72_bundler_project_tracer_cli -- --ignored
```
