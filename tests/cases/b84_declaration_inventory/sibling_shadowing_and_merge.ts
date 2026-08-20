// WU3b acceptance: same-scope fragments merge; sibling block groups remain distinct.

interface Merged {
  first: string;
}

interface Merged {
  second: number;
}

const missingMerged: Merged = { first: "ready" }; // error[TK2741]

{
  interface Shadowed {
    left: string;
  }
  const invalidLeft: Shadowed = { left: 1 }; // error[TK2322]
}

{
  interface Shadowed {
    right: number;
  }
  const invalidRight: Shadowed = { right: "wrong" }; // error[TK2322]
}

// WU3c acceptance: the class fragment attaches exactly once regardless of merge order.
interface InterfaceFirst<T> {
  interfaceValue: T;
}
class InterfaceFirst<T> {
  classValue!: T;
}
const invalidInterfaceFirst: InterfaceFirst<string> = {
  interfaceValue: "ready",
  classValue: 1, // error[TK2322]
};

class ClassFirst<T> {
  classValue!: T;
}
interface ClassFirst<T> {
  interfaceValue: T;
}
const invalidClassFirst: ClassFirst<string> = {
  interfaceValue: "ready",
  classValue: 1, // error[TK2322]
};
