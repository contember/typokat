// b06: what counts as implementing an abstract member — a concrete accessor and an
// initializer-inferred field both do; an INCOMPATIBLE implementation satisfies the
// completeness check but fails override compatibility (TK2416, not TK2515).

abstract class AV {
  abstract get acc(): number;
  abstract fn(): string;
}

class ImplVia extends AV {
  get acc(): number { return 1; }
  fn = () => "s";
}

class MissingBoth extends AV {} // error[TK2654]: Non-abstract class 'MissingBoth' is missing implementations for the following members of 'AV': 'acc', 'fn'

abstract class AInc {
  abstract go(): number;
}

class BadImpl extends AInc {
  go(): string { return "s"; } // error[TK2416]: Property 'go' in type 'BadImpl' is not assignable to the same property in base type 'AInc'
}
