// tsc 6.0.3 --strict --target es2025: TS2322 x2 below.

class M13PrivateSourceMember {
  private secret = 1;
}

class M13ProtectedSourceMember {
  protected secret = 1;
}

class M13PublicSourceMember {
  secret = 1;
}

declare const m13PrivateSource: M13PrivateSourceMember;
declare const m13ProtectedSource: M13ProtectedSourceMember;
declare const m13PublicSource: M13PublicSourceMember;
const m13PublicTargetFromPrivate: { secret: number } = m13PrivateSource; // error[TK2322]
const m13PublicTargetFromProtected: { secret: number } = m13ProtectedSource; // error[TK2322]
const m13PublicTargetFromPublic: { secret: number } = m13PublicSource;
