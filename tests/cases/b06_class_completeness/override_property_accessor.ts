// b06: TK2416 applies to plain properties and accessor overrides too.
// tsc additionally reports TS2426 (accessor-vs-function kind mismatch) on the
// DerivedAcc case — deferred, typokat reports only the TK2416 type incompatibility.

class BaseP {
  p: number = 1;
  get acc(): number { return 1; }
}

class DerivedP extends BaseP {
  p: string = "s"; // error[TK2416]: Property 'p' in type 'DerivedP' is not assignable to the same property in base type 'BaseP'
}

class DerivedAcc extends BaseP {
  acc(): number { return 2; } // error[TK2416]
}

class CleanP extends BaseP {
  p: number = 2;
  get acc(): number { return 3; }
}
