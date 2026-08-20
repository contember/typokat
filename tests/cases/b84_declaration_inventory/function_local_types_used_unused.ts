// WU3b parity: canonical inventory covers function-local alias/enum terminals.
// WU3c parity: function-local classes attach and check to completion.

function checkFunctionLocals(): void {
  type UsedAlias = { value: string };
  type UnusedAlias = { unused: true };
  const invalidAlias: UsedAlias = { value: 1 }; // error[TK2322]

  class UsedClass { // WU3c RED: class attachment consumes the WU3b terminal.
    value!: string;
  }
  class UnusedClass {}
  const invalidClass: UsedClass = { value: 1 }; // error[TK2322]

  enum UsedEnum { // incomplete[decl/enum-declaration/self]
    Ready,
  }
  enum UnusedEnum { // incomplete[decl/enum-declaration/self]
    Ready,
  }
  const ready: UsedEnum = UsedEnum.Ready;
}

checkFunctionLocals();
