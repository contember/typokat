// tsc 6.0.3 --strict: only the number assignment reports TS2322.

type RecursiveSibling = { self: RecursiveSibling };
type FreshenAlongsideRecursive<T> =
    T extends infer U ? { recursive: RecursiveSibling; value: U } : never;
type FreshenedSibling = FreshenAlongsideRecursive<"ok">;

declare const freshenedSibling: FreshenedSibling;
const acceptedSibling: "ok" = freshenedSibling.value;
const rejectedSibling: number = freshenedSibling.value; // error[TK2322]
