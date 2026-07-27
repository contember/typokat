#!/usr/bin/env python3
"""
Automatic shrinking — the step that turns a finding into a report someone can act on.

The adversarial review of `412f321` found the bug in minutes but only became
*actionable* when its 40-line generated program reduced to three lines with no
generics (backlog `95`). This module automates that reduction.

Two layers, in order:

  1. **Structural** (`shrink_program`) — delta-debugging over the generated tree, not
     over text. `grammar.program_reductions` enumerates the one-step reductions that
     keep a program well-formed: drop a statement, splice out a nesting level, drop an
     overload signature, de-genericise an arrow, shorten a projection (`p0 + 1` ->
     `p0`), drop the class/`this` wrapper, prune declarations the body stopped calling.
     Greedy: take the first reduction the oracle accepts, restart, until nothing
     shrinks. This is why the generator builds a tree instead of emitting text.

  2. **Textual** (`shrink_text`) — final polish that structure cannot express: strip
     the generator's origin comment, remove the batch-uniqueness name prefix, and drop
     any line that turns out not to matter. Line dropping also handles hand-written
     inputs, which have no tree.

The oracle is supplied by the caller and answers exactly one question: *does this
source still show the SAME divergence?* Same, not any — see `make_oracle` in
`differential.py`.
"""

import re
from typing import Callable, Optional, Tuple

from grammar import Program, program_reductions, prune_decls

Oracle = Callable[[str], bool]


def _size(prog: Program) -> int:
    return len(prog.render())


def shrink_program(prog: Program, oracle: Oracle, budget: int = 600) -> Tuple[Program, int]:
    """Greedy fixpoint reduction. Returns the smallest program the oracle still
    accepts, plus the number of oracle calls spent."""
    best = prune_decls(prog)
    calls = 0
    changed = True
    while changed and calls < budget:
        changed = False
        for cand in program_reductions(best):
            if calls >= budget:
                break
            if _size(cand) >= _size(best):
                continue
            calls += 1
            if oracle(cand.render()):
                best = cand
                changed = True
                break
    return best, calls


ORIGIN_RE = re.compile(r"^// differential: .*$", re.MULTILINE)


def shrink_text(source: str, oracle: Oracle, prefix: Optional[str] = None) -> str:
    """Textual polish. Every step is oracle-guarded, so a step that would change the
    finding is simply not taken."""
    best = source

    stripped = ORIGIN_RE.sub("", best).lstrip("\n")
    if stripped != best and oracle(stripped):
        best = stripped

    if prefix:
        unprefixed = best.replace(prefix, "")
        if unprefixed != best and oracle(unprefixed):
            best = unprefixed

    # Bottom-up so indices stay valid as lines disappear.
    changed = True
    while changed:
        changed = False
        lines = best.split("\n")
        for i in range(len(lines) - 1, -1, -1):
            if not lines[i].strip():
                continue
            cand = "\n".join(lines[:i] + lines[i + 1:])
            if oracle(cand):
                best = cand
                changed = True
                break

    return best if best.endswith("\n") else best + "\n"
