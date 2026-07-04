// b06 (backlog 06): TK2416 — an overriding member must be assignable to the base member.
// tsc 6.0.3 --strict cross-checked (sprint 2026-07-04 probes). The bivariant-only
// method-param-narrowing shape is deliberately absent (documented over-report divergence).

class Base {
  m(x: string): number { return 1; }
}

class Derived extends Base {
  m(x: string): string { return x; } // error[TK2416]: Property 'm' in type 'Derived' is not assignable to the same property in base type 'Base'
}

class Derived2 extends Base {
  m(x: number): number { return x; } // error[TK2416]: Property 'm' in type 'Derived2' is not assignable to the same property in base type 'Base'
}

class Compatible extends Base {
  m(x: string): number { return 2; }
}
