// M0 — strictNullChecks ON (our default): null and undefined are distinct types,
// each only assignable to itself, any, and unknown (undefined also to void).

const n_ok: null = null;
const u_ok: undefined = undefined;
const v_ok: void = undefined;

const bad1: number = null;      // error[TK2322]: Type 'null' is not assignable to type 'number'
const bad2: string = undefined; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
const bad3: null = undefined;   // error[TK2322]: Type 'undefined' is not assignable to type 'null'
