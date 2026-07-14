# Semantic-duplication project fixtures

This tree is intentionally **not runnable yet**. It is not registered in either
`MILESTONE_DIRS` or `PROJECT_DIRS` in `tests/conformance.rs`. `MILESTONE_DIRS`-only registration
fails loudly because this root contains zero immediate fixture files, while `PROJECT_DIRS`-only
registration is unreachable. The integration implementation must add `sr_semantic_duplication_project`
to both tables in the same change that makes these fixtures pass.

Each immediate child directory is one project following the M29 convention. Files are sorted before
they become `FileInput`s, so `opposite_order/00_derived.ts` deliberately precedes—and imports—
`99_base.ts`. The driver's dependency order is therefore the reverse of its stable input/module order.

Cross-check the project with:

```sh
tsc --strict --noEmit --module esnext --moduleResolution bundler \
  tests/cases/sr_semantic_duplication_project/opposite_order/00_derived.ts \
  tests/cases/sr_semantic_duplication_project/opposite_order/99_base.ts
```

Cyclic module graphs remain out of scope. The exported poisoned base and the unexported local
heritage cycle both live in `99_base.ts`; `00_derived.ts` reaches only the exported poisoned base
through an acyclic import edge, not the local cycle.
