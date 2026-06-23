// M1 — name resolution. Unresolved identifiers are TK2304 and get the error type,
// which suppresses any cascade (no secondary TK2322 on the same expression).

const base: number = 1;
const usesBase: number = base; // resolves to number -> ok

const usesMissing: number = ghost; // error[TK2304]: Cannot find name 'ghost'
