// Initially disabled WU6A soundness-review oracle: tsc 6.0.3 --strict --noEmit --pretty false
// --lib es5 --module commonjs. Private value declarations stay off the public namespace payload,
// but consuming the standalone fragment must retain each checker's exact unsupported owner.

namespace Wu6aReviewPrivateImportSource {
  export const value: number = 1;
}

namespace Wu6aReviewPrivateUnsupportedDeclarations {
  export const ready: number = 1;
  enum HiddenMode { A } // incomplete[decl/enum-declaration/self]
  import HiddenAlias = Wu6aReviewPrivateImportSource; // incomplete[decl/import-equals/self]
}

const wu6aReviewPrivateReady: number = Wu6aReviewPrivateUnsupportedDeclarations.ready;
Wu6aReviewPrivateUnsupportedDeclarations.HiddenMode; // error[TK2339]: Property 'HiddenMode' does not exist
Wu6aReviewPrivateUnsupportedDeclarations.HiddenAlias; // error[TK2339]: Property 'HiddenAlias' does not exist
