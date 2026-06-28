// F1 / backlog 05 (WU3) - class values expose a construct signature only when
// their constructor is publicly constructable.
// Cross-checked against tsc 6.0.3 --strict.

class PrivateCtor {
  private constructor() {}
  value: number = 1;
}

class ProtectedCtor {
  protected constructor() {}
  value: number = 1;
}

class PublicCtor {
  constructor() {}
  value: number = 1;
}

interface PrivateCtorInterface {
  new (): PrivateCtor;
}

interface ProtectedCtorInterface {
  new (): ProtectedCtor;
}

interface PublicCtorInterface {
  new (): PublicCtor;
}

type PrivateCtorObject = {
  new (): PrivateCtor;
};

type ProtectedCtorObject = {
  new (): ProtectedCtor;
};

type PublicCtorObject = {
  new (): PublicCtor;
};

const privateToNewType: new () => PrivateCtor = PrivateCtor;                 // error[TK2322]
const privateToInterface: PrivateCtorInterface = PrivateCtor;                // error[TK2322]
const privateToObject: PrivateCtorObject = PrivateCtor;                      // error[TK2322]

const protectedToNewType: new () => ProtectedCtor = ProtectedCtor;           // error[TK2322]
const protectedToInterface: ProtectedCtorInterface = ProtectedCtor;          // error[TK2322]
const protectedToObject: ProtectedCtorObject = ProtectedCtor;                // error[TK2322]

const publicToNewType: new () => PublicCtor = PublicCtor;
const publicToInterface: PublicCtorInterface = PublicCtor;
const publicToObject: PublicCtorObject = PublicCtor;
