// M27 — template PATTERNS (holes of string/number) in the relation engine:
// a string literal matches a pattern by anchored segment matching; `string`
// itself does NOT match a pattern with any literal text; a pattern is
// assignable to `string` and to a subsuming pattern. tsc 6.0.3 --strict
// cross-checked. Template displays are unstable — code-only markers.

type Greeting = `hello ${string}`;
const g1: Greeting = "hello world";
const g2: Greeting = "hello ";
const g3: Greeting = "goodbye world"; // error[TK2322]

// `${number}` hole requires a numeric segment.
type NumHole = `n${number}`;
const n1: NumHole = "n42";
const n2: NumHole = "n3.5";
const n3: NumHole = "nx"; // error[TK2322]
// tsc accepts all parseable number spellings; backlog 63(e) owns these safe FPs.
const n4: NumHole = "n01"; // error[TK2322]
const n5: NumHole = "n1e3"; // error[TK2322]

// Multiple wide holes: anchored on the literal separator.
type WideHole = `${string}-${string}`;
const w1: WideHole = "a-b";
const w2: WideHole = "-";
const w3: WideHole = "ab"; // error[TK2322]

// string itself does not match a pattern…
declare const someStr: string;
const s1: Greeting = someStr; // error[TK2322]
// …but a pattern flows into string and into a subsuming pattern.
declare const hp: `hello ${string}`;
const s2: string = hp;
const s3: `${string}` = hp;
const s4: `h${string}` = hp;
const s5: `x${string}` = hp; // error[TK2322]
