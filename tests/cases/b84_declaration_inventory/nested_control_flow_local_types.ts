// WU3b acceptance: control-flow-local groups use binder fragment order.

declare const condition: boolean;

if (condition) {
  type IfLocal = { value: string };
  type UnusedIfLocal = { unused: true };
  const invalid: IfLocal = { value: 1 }; // error[TK2322]
}

switch (condition) {
  case true: {
    type SwitchLocal = { value: string };
    type UnusedSwitchLocal = { unused: true };
    const invalid: SwitchLocal = { value: 1 }; // error[TK2322]
    break;
  }
}

while (condition) {
  class LoopLocal {
    value!: string;
  }
  class UnusedLoopLocal {}
  const invalid: LoopLocal = { value: 1 }; // error[TK2322]
  break;
}

try {
  throw 1;
} catch {
  class CatchLocal {
    value!: string;
  }
  class UnusedCatchLocal {}
  const invalid: CatchLocal = { value: 1 }; // error[TK2322]
}

if (condition) {
  enum IfEnum { // incomplete[decl/enum-declaration/self]
    Ready,
  }
  enum UnusedIfEnum { // incomplete[decl/enum-declaration/self]
    Ready,
  }
  const ready: IfEnum = IfEnum.Ready;
}
