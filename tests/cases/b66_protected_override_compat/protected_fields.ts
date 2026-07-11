// Backlog 66 — protected data properties use the strict override relation.
// Cross-checked with tsc 6.0.3 --strict.

class FieldBase {
  protected value: string = "base";
  protected stable: number = 1;
}

class GoodFields extends FieldBase {
  protected value: string = "derived";
  protected stable: number = 2;
}

class BadFields extends FieldBase {
  protected value: number = 1; // error[TK2416]
  protected stable: number = 2;
}
