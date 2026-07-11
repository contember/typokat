// Cross-checked with tsc 6.0.3 --strict (TS1361 + TS2345; TK2304 is the local stand-in).

import type { Math } from "./type_middle";

Math.abs(1); // error[TK2304]: Cannot find name 'Math'
