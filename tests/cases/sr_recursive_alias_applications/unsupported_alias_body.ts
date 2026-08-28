// A failed alias body stays unavailable; it must never become a Ready error body.
type UnsupportedAlias<T> = T extends typeof unsupportedAliasValue ? T : never; // incomplete[annotation-lower/type-query/typeof]: typeof type query not lowered
declare const unsupportedAlias: UnsupportedAlias<string>;
const unsupportedAliasAssignment: number = unsupportedAlias;
