// The type export remains usable, while its value use stays absent.

import { M } from "./a";

const typed: M = { value: 1 };
const value = M; // error[TK2304]: Cannot find name 'M'
