// tsc 6.0.3 --strict: clean; update traversal must retain nested unsupported identities.

let n = 1;

(n!)++; // incomplete[expr-infer/non-null-assertion/self]
(n satisfies number)++; // incomplete[expr-infer/satisfies-expression/self]
((n satisfies number)!)++; // incomplete[expr-infer/satisfies-expression/self] | incomplete[expr-infer/non-null-assertion/self]

class C {
  #x = 0;

  update() { this.#x++; } // incomplete[expr-infer/private-field-access/self]
}
