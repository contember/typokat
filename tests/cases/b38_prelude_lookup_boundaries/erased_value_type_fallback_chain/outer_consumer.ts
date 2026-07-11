// The imported name has no type slot, so ambient Partial remains reachable.
// Cross-checked with tsc 6.0.3 --strict.

import type { Partial } from "./outer_middle";

type Outer = Partial<{ value: string }>;
const outer: Outer = {};
