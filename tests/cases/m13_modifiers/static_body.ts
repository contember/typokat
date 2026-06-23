// M13 — static method bodies and static field initializers are type-checked too
// (a static member's body is checked like any function; `this` there is the class
// value, not an instance).

function helper(x: number): void {}

class C {
  static label: number = missingName; // error[TK2304]: Cannot find name 'missingName'
  value: number;
  constructor() {
    this.value = 0;
  }

  static good(): number {
    return 1; // ok
  }
  static bad(): number {
    return "s"; // error[TK2322]
  }
  static useMissing(): void {
    const z = nope; // error[TK2304]: Cannot find name 'nope'
  }
  static callBad(): void {
    helper("s"); // error[TK2345]
  }
}
