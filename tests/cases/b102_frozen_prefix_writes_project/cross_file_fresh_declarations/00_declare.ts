// Backlog 102: every remaining fresh script-scope declaration form, so the fix is not narrowed
// to `var`/`interface`. All four names are absent from the default library.
declare function b102CrossFn(input: number): string;

declare namespace B102CrossNamespace {
  const member: number;
}

class B102CrossClass {
  value: number = 1;
}

type B102CrossAlias = string;
