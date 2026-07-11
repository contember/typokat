// Backlog 66 scope controls. Public overrides retain the shipped TK2416 path.
// Visibility narrowing is TS2415 territory and remains deliberately out of scope.

class PublicBase {
  public method(value: string): void {}
}

class PublicBad extends PublicBase {
  public method(value: number): void {} // error[TK2416]
}

class MixedVisibility extends PublicBase {
  // tsc: TS2415 on the class; typokat deliberately leaves visibility narrowing deferred.
  protected method(value: string): void {}
}

class PrivateBase {
  private method(value: string): void {}
}

class PrivateRedeclaration extends PrivateBase {
  // tsc: TS2416 here; private redeclaration remains outside this protected-pair slice.
  private method(value: number): void {}
}
