// b06 (review round 1): the method-bivariance rule is keyed on the BASE member's
// declaration kind, not the derived one's (tsc 6.0.3, probed). Base FIELD → strict
// contravariance regardless of how the derived member is declared; base METHOD →
// bivariant params regardless of the derived kind. tsc additionally reports TS2425
// (kind mismatch) on field↔method mixtures — deferred, like TS2426.

class BaseF {
  m: (x: string | number) => void = () => {};
}
class DerivedM extends BaseF {
  m(x: string): void {} // error[TK2416]: Property 'm' in type 'DerivedM' is not assignable to the same property in base type 'BaseF'
}

class BaseF2 {
  cb: () => void = () => {};
}
class DerivedM2 extends BaseF2 {
  cb(x: string): void {} // error[TK2416]
}

class BaseM {
  m(x: string | number): void {}
}
class DerivedF extends BaseM {
  m: (x: string) => void = () => {};
}

class BaseF3 {
  m: (x: string) => void = () => {};
}
class DerivedMOk extends BaseF3 {
  m(x: string): void {}
}
