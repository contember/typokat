import type { T } from "./a";
import { T as U } from "./a";

const fromTypeOnly: number = T; // error[TK2304]: Cannot find name 'T'
const fromTypeExport: number = U; // error[TK2304]: Cannot find name 'U'
