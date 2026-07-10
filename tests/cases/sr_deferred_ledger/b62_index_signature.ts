// Deferred ledger / backlog 62 — `relate_objects` has no "source must provide an
// index signature" rule, so a declared (interface/class) source is wrongly
// accepted against an index-signature target. tsc grants an implicit index
// signature only to ANONYMOUS object types, so it reports TS2322 "Index signature
// … is missing in type 'I'". This corpus stays DISABLED beyond this sprint (until
// backlog 62 adds the implicit-index rule). Cross-checked vs tsc 6.0.3 --strict.
// Asserted code-only (index-signature target).

interface Iface {
  a: number;
}
declare const iv: Iface;

// witness (dropped error): a nominal interface source has no implicit index
// signature — tsc: TS2322. typokat is silent today.
const dropped: { [x: string]: number } = iv; // error[TK2322]

// --- control: an ANONYMOUS object source gets an implicit index signature and
// is accepted (already correct). ---
declare const anon: { a: number };
const ok: { [x: string]: number } = anon;
