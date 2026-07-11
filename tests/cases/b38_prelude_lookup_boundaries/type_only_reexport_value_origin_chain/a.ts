// The original type-only export erases C's static value side.

class C {
  static abs(value: string): string {
    return value;
  }
}

export type { C as Math };
