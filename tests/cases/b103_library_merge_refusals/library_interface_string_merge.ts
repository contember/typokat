// Backlog 103, the guard tier. The same refusal on a primitive wrapper interface: the frozen
// `String` group cannot take a user fragment, so the declaration is recorded instead of
// crashing the binder.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean — the merge is legal. typokat
// refuses it and over-reports TK2339; ledgered in docs/reference/divergences.md under backlog 103.
interface String { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  b103Upper(): string;
}

const shouted: string = "quiet".b103Upper(); // error[TK2339]: Property 'b103Upper' does not exist
