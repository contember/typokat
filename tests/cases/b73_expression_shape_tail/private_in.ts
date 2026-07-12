// tsc 6.0.3 --strict: TS2304 on the right-hand expression.

class C {
  #x = 0;

  has() { return #x in Missing; } // error[TK2304]: Cannot find name 'Missing' | incomplete[expr-infer/private-in-expression/self]
}
