// WU2 / finding 4 (project-shaped) — `collect_list_export` (mod.rs:495) ignores
// `export_kind`, so the SPECIFIER forms `export type { x }` and `export { type x }`
// leak the VALUE slot of `x`. A non-type-only import can then use `x` as a
// runtime value, which tsc rejects (TS1362 "cannot be used as a value because it
// was exported using 'export type'"). typokat has no TS1362 code; the M29 model
// maps a type-only export used as a value to TK2304 (see divergences / the m29
// `type_only_value` fixture). The DECLARATION forms (`export interface`,
// `export type X = …`) are already handled correctly and are positive controls.
// Cross-checked vs tsc 6.0.3 --strict (files compiled together).

const valA = 1;
export type { valA }; // type-only specifier export → value slot must NOT leak

const valB = 2;
export { type valB }; // inline type-only specifier export → same

// controls: real value export + declaration-form type exports.
export const realVal = 3;
export interface IShape {
  n: number;
}
export type Alias = number;
