// A type-only import of a type-only re-exported class must not reach ambient Math.
// Cross-checked with tsc 6.0.3 --strict (TS1361 + TS2345; TK2304 is the local stand-in).

import type { Math, InlineMath } from "./a";

Math.abs(1); // error[TK2304]: Cannot find name 'Math'
InlineMath.abs(1); // error[TK2304]: Cannot find name 'InlineMath'
