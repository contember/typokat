// Deferred ledger / backlog 75 — a dependent indexed access in a generic callback
// (`Events[K]`) loses the key-to-payload correlation before call-site inference. tsc
// 6.0.3 --strict accepts the selected-key callbacks below; typokat conservatively
// rejects them after contextual generic callback typing. This corpus stays disabled
// until the deferred generic indexed-access model can retain and evaluate the pair.

interface Events {
  text: [string];
  count: [number];
}

declare function on<K extends keyof Events>(
  key: K,
  callback: (...args: Events[K]) => void,
): void;
declare function acceptsText(value: string): void;
declare function acceptsCount(value: number): void;

// tsc-clean witnesses: each literal key selects its own callback tuple.
on("text", value => acceptsText(value));
on("count", value => acceptsCount(value));

// Controls: mismatched selected-key callback parameters remain errors in tsc.
on("text", (value: number) => {}); // error[TK2345]
on("count", (value: string) => {}); // error[TK2345]
