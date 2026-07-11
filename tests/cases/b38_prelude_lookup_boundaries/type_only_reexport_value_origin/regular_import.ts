// A regular import cannot restore the value erased by either type-only export form.
// Cross-checked with tsc 6.0.3 --strict (TS1362; TK2304 is the local stand-in).

import { Math, InlineMath } from "./a";

Math.abs(1); // error[TK2304]: Cannot find name 'Math'
InlineMath.abs(1); // error[TK2304]: Cannot find name 'InlineMath'
