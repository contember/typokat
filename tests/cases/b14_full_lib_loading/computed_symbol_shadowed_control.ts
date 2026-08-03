// tsc 6.0.3 --strict --target es2025: clean. The module-local Symbol must not acquire
// the default library's certified semantic identity even though its member spelling matches.

export {};

declare const Symbol: {
  iterator: "module-local";
};

interface B14ShadowedSymbolControl {
  [Symbol.iterator](): void; // incomplete[signature/method-signature/computed-key]
}
