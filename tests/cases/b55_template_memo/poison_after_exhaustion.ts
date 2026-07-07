// backlog 55: a template-literal hole evaluated AFTER the shared TK2589 budget is
// exhausted (by an unrelated member of the same root evaluation) must not be
// memoized as the template's final result. At the buggy HEAD the `later` line is
// silent: the error-valued template is committed to the pass-wide memo and the
// hash-consed node poisons every later annotation.
type Inf<T> = T extends {} ? Inf<{ v: T }> : never;
type Seq<T> = Inf<T> extends never ? `a${Uppercase<"b">}` : `a${Uppercase<"b">}`;
type Fine<T> = T extends string ? `a${Uppercase<"b">}` : never;
const g: Seq<{}> = "x"; // error[TK2589]
const later: Fine<string> = "no"; // error[TK2322]: Type '"no"' is not assignable to type '"aB"'
const ok: Fine<string> = "aB";
