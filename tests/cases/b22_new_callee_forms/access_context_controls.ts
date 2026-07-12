// Backlog 22 — alias provenance must retain the lexical accessibility context.

class PrivateFactory {
  private constructor() {}

  static make(): PrivateFactory {
    const Alias = PrivateFactory;
    return new Alias();
  }
}

class ProtectedBase {
  protected constructor() {}
}

class ProtectedFactory extends ProtectedBase {
  static make(): ProtectedFactory {
    const Alias = ProtectedFactory;
    return new Alias();
  }
}
