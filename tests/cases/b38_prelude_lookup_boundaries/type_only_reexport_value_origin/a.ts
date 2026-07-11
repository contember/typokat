// Both type-only export forms retain the fact that C also has a value slot.

class C {
  static abs(value: string): string {
    return value;
  }
}

export type { C as Math };
export { type C as InlineMath };
