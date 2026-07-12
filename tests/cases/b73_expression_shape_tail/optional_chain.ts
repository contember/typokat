// tsc 6.0.3 --strict: TS2304; typokat must also expose optional-chain semantics.

Missing?.value; // error[TK2304]: Cannot find name 'Missing' | incomplete[expr-infer/optional-chain/self]

declare const maybe: { value: number } | undefined;

maybe?.value; // incomplete[expr-infer/optional-chain/self]
