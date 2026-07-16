// WU6A adversarial oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// The missing qualified annotation reports TS2694. A direct terminal inspector must additionally
// prove that typokat does not publish the root from the surviving `good` prefix.

namespace Wu6aReviewMissingChildRoot {
  export const good: number = 1;
  export const broken: Wu6aReviewMissingChildRoot.Missing = 1; // error[TK2694]: Namespace 'Wu6aReviewMissingChildRoot' has no exported member 'Missing'
}

const wu6aReviewMissingChildAlias = Wu6aReviewMissingChildRoot;
const wu6aReviewMissingChildStructural: {
  readonly good: number;
  readonly broken: number;
} = Wu6aReviewMissingChildRoot;
