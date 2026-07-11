// A type-only import of a type-only export must not hide the ambient Math value.
// Cross-checked with tsc 6.0.3 --strict.

import type { Math } from "./a";

const typed: Math = { local: true };
const absolute: number = Math.abs(-1);
