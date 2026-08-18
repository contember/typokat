// WU3a acceptance: nested external-module interfaces keep exact lexical owners.

export {};

declare const condition: boolean;

{
  interface BlockLocal {
    value: string;
  }
  const invalid: BlockLocal = { value: 1 }; // error[TK2322]
}

if (condition) {
  interface IfLocal {
    value: string;
  }
  const invalid: IfLocal = { value: 1 }; // error[TK2322]
}

switch (condition) {
  case true: {
    interface SwitchLocal {
      value: string;
    }
    const invalid: SwitchLocal = { value: 1 }; // error[TK2322]
    break;
  }
}

while (condition) {
  interface LoopLocal {
    value: string;
  }
  const invalid: LoopLocal = { value: 1 }; // error[TK2322]
  break;
}

try {
  throw 1;
} catch {
  interface CatchLocal {
    value: string;
  }
  const invalid: CatchLocal = { value: 1 }; // error[TK2322]
}
