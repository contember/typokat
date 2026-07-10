// WU2 / finding 4 — the consuming module. A plain (non-type-only) import of a
// type-only-exported name must not yield a usable runtime value. DISABLED at
// HEAD; enabling exposes the missing TK2304 on the value uses of `valA`/`valB`.
// Cross-checked vs tsc 6.0.3 --strict (TS1362 on both; typokat's M29 stand-in is
// TK2304).

import { valA, valB, realVal, IShape, Alias } from "./a";

// witnesses: type-only exported names used as values.
const useA: number = valA; // error[TK2304]: Cannot find name 'valA'
const useB: number = valB; // error[TK2304]: Cannot find name 'valB'

// controls: a real value export and the declaration-form type exports work.
const okReal: number = realVal;
const okShape: IShape = { n: 1 };
const okAlias: Alias = 5;
