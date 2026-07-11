// Backlog 66 — protected overrides use the existing method-bivariance and
// covariant-return rules. Cross-checked with tsc 6.0.3 --strict.

class Base {
  protected same(value: string): void {}
  protected parameter(value: string): void {}
  protected result(): { value: number } { return { value: 1 }; }
  protected badResult(): number { return 1; }
}

class Compatible extends Base {
  protected same(value: string): void {}
  protected parameter(value: string | number): void {}
  protected result(): { value: number; detail: string } {
    return { value: 1, detail: "ok" };
  }
  protected badResult(): number { return 1; }
}

class Incompatible extends Base {
  protected same(value: string): void {}
  protected parameter(value: number): void {} // error[TK2416]
  protected result(): { value: number; detail: string } {
    return { value: 1, detail: "ok" };
  }
  protected badResult(): string { return "bad"; } // error[TK2416]
}
