// tsc 6.0.3 --strict: the null leaf reports TS2322; evaluation terminates.

type RecursiveShape = { self: RecursiveShape };
type ExtractRecursive<T> = T extends infer U ? RecursiveShape : never;
type RecursiveResult = ExtractRecursive<"ok">;

const badRecursive: RecursiveResult = { self: null }; // error[TK2322]: null
