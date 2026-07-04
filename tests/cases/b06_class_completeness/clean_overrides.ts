// b06: compatible overrides must stay clean — identical signature, covariant
// return, required member over optional base member. No diagnostics expected.

class CBase {
  m(): string | number { return 1; }
  q?: string;
}

class COv extends CBase {
  m(): number { return 2; }
  q: string = "s";
}

class CSame extends CBase {
  m(): string | number { return "x"; }
}
