// tsc 6.0.3 --strict: TS2304 in the private-field base expression.

class C {
  #x = 1;

  read() { return Missing.#x; } // error[TK2304]: Cannot find name 'Missing' | incomplete[expr-infer/private-field-access/self]
}
