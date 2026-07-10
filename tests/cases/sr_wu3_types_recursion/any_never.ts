// WU3 / finding 7 — `any & never` normalizes to `any` (operators.rs:111); tsc
// keeps `never` (never is the annihilator of intersection). So a target of type
// `any & never` should reject anything a `never` target rejects, but typokat
// (treating it as `any`) accepts. Witness: assigning a string INTO `any & never`
// must error. DISABLED at HEAD; enabling exposes the missing TK2322.
// Cross-checked vs tsc 6.0.3 --strict (TS2322 to `never`).

let sink: any & never;
sink = "s"; // error[TK2322]: not assignable to type 'never'

// --- controls ---
// `never` still flows OUT (never is assignable to everything).
declare const readNever: any & never;
const flows: string = readNever;
// plain `any` is unaffected.
let plainAny: any;
plainAny = "s";
