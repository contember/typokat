// Private-field access is opaque until binder/private-member semantics are modeled.

class C {
  #x = 1;

  read() { return Missing.#x; } // incomplete[expr-infer/private-field-access/self]

  rest(other: C) {
    const { ...copy } = other;
    return copy.#x; // incomplete[expr-infer/private-field-access/self]
  }
}
