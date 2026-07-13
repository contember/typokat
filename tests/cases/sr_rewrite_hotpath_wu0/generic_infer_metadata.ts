// tsc 6.0.3 --strict: clean; the outer infer reaches every nested generic type child.

type NestedGeneric<T> =
    T extends { value: infer R }
        ? <U extends R = R>(this: R, value?: U, ...tail: U[]) => U
        : never;

declare const nestedGeneric: NestedGeneric<{ value: "ok" }>;
const preservedMetadata:
    <U extends "ok" = "ok">(this: "ok", value?: U, ...tail: U[]) => U = nestedGeneric;
