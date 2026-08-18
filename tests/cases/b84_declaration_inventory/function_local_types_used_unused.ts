// WU3b parity: canonical inventory covers used and unused function-local groups.

function checkFunctionLocals(): void {
  type UsedAlias = { value: string };
  type UnusedAlias = { unused: true };
  const invalidAlias: UsedAlias = { value: 1 }; // error[TK2322]

  class UsedClass {
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
