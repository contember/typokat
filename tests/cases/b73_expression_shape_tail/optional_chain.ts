// tsc 6.0.3 --strict: TS2304/TS2345; typokat exposes deferred optional-chain semantics.

Missing?.value; // error[TK2304]: Cannot find name 'Missing' | incomplete[expr-infer/optional-chain/self]

declare const maybe: { value: number } | undefined;

maybe?.value; // incomplete[expr-infer/optional-chain/self]

declare const nested: { value: { run(x: number): number } } | undefined;

nested?.value.run("bad"); // incomplete[expr-infer/optional-chain/self]
nested?.value[MissingKey]; // error[TK2304]: Cannot find name 'MissingKey' | incomplete[expr-infer/optional-chain/self]

declare const maybeFn: ((x: number) => number) | undefined;

maybeFn?.("bad"); // incomplete[expr-infer/optional-chain/self]
