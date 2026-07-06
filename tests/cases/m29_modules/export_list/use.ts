import { exportedNumber, LocalAlias, missing } from "./a"; // error[TK2305]: has no exported member

const bad: string = exportedNumber; // error[TK2322]: Type 'number' is not assignable to type 'string'
const badAlias: LocalAlias = { count: "x" }; // error[TK2322]: Type 'string' is not assignable to type 'number'
const ok: string = present; // error[TK2304]: Cannot find name 'present'
