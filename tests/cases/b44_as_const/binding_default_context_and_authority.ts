// A default contributes a type without replacing annotation authority or callback context.

declare const defaultSource: { value?: "bar"; cb?: (n: number) => number };
let { value: ordinaryBindingDefault = "foo" } = defaultSource;
ordinaryBindingDefault = "different";
ordinaryBindingDefault = 1; // error[TK2322]

const { value: constBindingDefault = "foo" } = defaultSource;
const constBindingChoice: "bar" | "foo" = constBindingDefault;
const constBindingWrong: "bar" = constBindingDefault; // error[TK2322]

let { value: assertedBindingDefault = "foo" as const } = defaultSource;
assertedBindingDefault = "bar";
assertedBindingDefault = "foo";
assertedBindingDefault = "different"; // error[TK2322]

let { cb: contextualDefault = (n) => {
  const wrongContext: string = n; // error[TK2322]
  return n;
} } = defaultSource;
contextualDefault("wrong"); // error[TK2345]

let { value: mixedBindingDefault = 123 } = defaultSource;
mixedBindingDefault = 456;
mixedBindingDefault = "bar";
mixedBindingDefault = "wrong"; // error[TK2322]

let { value: undefinedBindingDefault = "foo" } = { value: undefined };
undefinedBindingDefault = "different";
undefinedBindingDefault = 1; // error[TK2322]

let { value: annotatedBindingDefault = "foo" }: { value?: "bar" } = {}; // error[TK2322]
annotatedBindingDefault = "foo"; // error[TK2322]

declare const nullableDefaultSource: { value: "bar" | null | undefined };
let { value: nullableBindingDefault = "foo" as const } = nullableDefaultSource;
nullableBindingDefault = null;
nullableBindingDefault = "foo";
nullableBindingDefault = undefined; // error[TK2322]
