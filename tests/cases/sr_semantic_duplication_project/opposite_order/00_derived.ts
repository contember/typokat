// Project semantic-duplication gate — input order deliberately opposes dependency order.
// tsc 6.0.3 --strict reports its native cyclic-base diagnostics from 99_base.ts and accepts the
// initializer. typokat attributes this poisoned-base event to this derived module's extends site.

import { OrderedPoisonedBase } from "./99_base";

export class OrderedDerived extends OrderedPoisonedBase {} // incomplete[class/class-heritage/poisoned-base]
export class OrderedDerivedAgain extends OrderedDerived {} // incomplete[class/class-heritage/poisoned-base]

const earlierDerivedDiagnostic: string = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
const laterDerivedDiagnostic: number = "later"; // error[TK2322]: Type 'string' is not assignable to type 'number'
