// WU3b acceptance: script-continuation block groups become terminal before checking.

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
