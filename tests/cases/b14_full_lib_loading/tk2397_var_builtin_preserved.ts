var globalThis: number; // error[TK2397]: Declaration name conflicts with built-in global identifier 'globalThis'.

const absolute: number = globalThis.Math.abs(-1);
