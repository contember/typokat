function readsNestedEnum(): void {
  enum LocalMode { // incomplete[decl/enum-declaration/self]
    Ready,
  }
  const ready: LocalMode = LocalMode.Ready;
}
