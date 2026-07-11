// Backlog 66 — protected overrides compare nested protected members along their
// class lineage, not as unrelated nominal origins. Cross-checked with tsc 6.0.3 --strict.

class ValueBase {
  protected brand: string = "base";
}

class ValueDerived extends ValueBase {
  protected brand: string = "derived";
}

class HolderBase {
  protected field: ValueBase = new ValueBase();
  protected result(): ValueBase { return new ValueBase(); }
  protected parameter(value: ValueBase): void {}
}

class HolderDerived extends HolderBase {
  protected field: ValueDerived = new ValueDerived();
  protected result(): ValueDerived { return new ValueDerived(); }
  protected parameter(value: ValueDerived): void {}
}
