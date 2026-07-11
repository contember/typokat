// Cross-checked with tsc 6.0.3 --strict (TS1362 + TS2345; TK2304 is the local stand-in).

import { Math } from "./regular_second";

Math.abs(1); // error[TK2304]: Cannot find name 'Math'
