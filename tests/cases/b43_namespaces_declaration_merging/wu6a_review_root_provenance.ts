// WU6A adversarial oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// Namespace provenance survives ordinary aliases and parentheses, but an explicit `any` annotation
// deliberately erases it and retains normal any call/construct behavior.

namespace Wu6aReviewRootProvenance {
  export const value: number = 1;
}

const wu6aReviewAlias = Wu6aReviewRootProvenance;
const wu6aReviewChainedAlias = wu6aReviewAlias;

(Wu6aReviewRootProvenance)(); // error[TK2349]: This expression is not callable
new (Wu6aReviewRootProvenance)(); // error[TK2351]: This expression is not constructable
wu6aReviewAlias(); // error[TK2349]: This expression is not callable
new wu6aReviewAlias(); // error[TK2351]: This expression is not constructable
(wu6aReviewChainedAlias)(); // error[TK2349]: This expression is not callable
new (wu6aReviewChainedAlias)(); // error[TK2351]: This expression is not constructable

const wu6aReviewConstAny: any = Wu6aReviewRootProvenance;
let wu6aReviewLetAny: any = Wu6aReviewRootProvenance;
var wu6aReviewVarAny: any = Wu6aReviewRootProvenance;

wu6aReviewConstAny();
new wu6aReviewConstAny();
wu6aReviewLetAny();
new wu6aReviewLetAny();
wu6aReviewVarAny();
new wu6aReviewVarAny();
