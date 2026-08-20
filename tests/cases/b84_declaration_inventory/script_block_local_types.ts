// WU3b acceptance: script-continuation interface/alias groups become terminal before checking.
// WU3c acceptance: local classes attach to the WU3b terminal and check to completion.

{
  interface LocalInterface {
    value: string;
  }
  const valid: LocalInterface = { value: "valid" };
  const invalid: LocalInterface = { value: 1 }; // error[TK2322]

  interface UnusedLocalInterface {
    unused: true;
  }
}

{
  type LocalAlias = { value: string };
  const valid: LocalAlias = { value: "valid" };
  const invalid: LocalAlias = { value: 1 }; // error[TK2322]

  type UnusedLocalAlias = { unused: true };
}

{
  // WU3c: the canonical class fragment attaches to its reserved terminal.
  class LocalClass {
    value!: string;
  }
  const valid: LocalClass = { value: "valid" };
  const invalid: LocalClass = { value: 1 }; // error[TK2322]

  class UnusedLocalClass {}
}

{
  enum LocalEnum { // incomplete[decl/enum-declaration/self]
    Ready,
  }
  const ready: LocalEnum = LocalEnum.Ready;

  enum UnusedLocalEnum { // incomplete[decl/enum-declaration/self]
    Ready,
  }
}
