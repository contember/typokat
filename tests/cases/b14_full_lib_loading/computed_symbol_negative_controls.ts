// tsc 6.0.3 --strict --target es2025: TS1169 x1 on the arbitrary call expression;
// Symbol.isConcatSpreadable is clean but remains outside the certified lowering set.

declare function b14ArbitraryComputedKey(): "arbitrary";
interface B14ArbitraryComputedControl {
  [b14ArbitraryComputedKey()](): void; // incomplete[signature/method-signature/computed-key]
}

interface B14NonCertifiedWellKnownControl {
  [Symbol.isConcatSpreadable](): void; // incomplete[signature/method-signature/computed-key]
}
