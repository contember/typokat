// A type-only import of a value-bearing export blocks the ambient Math value.
// Cross-checked with tsc 6.0.3 --strict (TS1361 + TS2345; TK2304 is the local stand-in).

import type { Math } from "./a";

const typed: Math = {};
Math.abs(1); // error[TK2304]: Cannot find name 'Math'
