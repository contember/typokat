// tsc 6.0.3 --strict: the top-level operator owns TS1360, while contextual member
// errors and fresh excess properties keep their established codes.

"wrong" satisfies number; // error[TK1360]

({ value: 1 }) satisfies { value: number; missing: string }; // error[TK1360]

[1] satisfies [number, number]; // error[TK1360]

({ value: "wrong" }) satisfies { value: number }; // error[TK2322]: Type 'string' is not assignable to type 'number'

({ value: 1, extra: true }) satisfies { value: number }; // error[TK2353]

({ nested: { value: 1, extra: true } }) satisfies { nested: { value: number } }; // error[TK2353]

declare const wholeUnion: { kind: "a"; value: string } | { kind: "b"; value: boolean };
wholeUnion satisfies { kind: "a" | "b"; value: number }; // error[TK1360]
