// Deferred ledger / backlog 30 — `number_to_string` uses plain decimal
// formatting, not JS `String(n)`: `` `${1e21}` `` constructs
// "1000000000000000000000" where tsc's type is "1e+21". typokat therefore
// ACCEPTS the non-canonical digit string tsc rejects — a real UNDER-report
// (dropped error). This corpus stays DISABLED beyond this sprint (until backlog
// 30 ships JS-exact stringification). Cross-checked vs tsc 6.0.3 --strict.

type Big = `${1e21}`; // tsc: the literal type "1e+21"

// witness (dropped error): the non-canonical form must NOT be assignable —
// tsc: TS2322. typokat is silent today (its Big is the digit string).
const dropped: Big = "1000000000000000000000"; // error[TK2322]

// --- control: a small magnitude round-trips identically in both. ---
type Small = `${5}`;
const ok: Small = "5";
