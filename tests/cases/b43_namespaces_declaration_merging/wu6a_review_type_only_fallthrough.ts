// WU6A adversarial oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// A nested type-only namespace does not occupy value space and therefore does not block the outer
// number binding. The bare read is clean; call/new diagnose the outer number, never TK2708.

const Wu6aReviewOuterNumber = 1;

namespace Wu6aReviewTypeOnlyHost {
  export const live: number = 1;

  export namespace Wu6aReviewOuterNumber {
    export interface Shape {
      value: number;
    }
  }

  export function exercise(): void {
    Wu6aReviewOuterNumber;
    Wu6aReviewOuterNumber(); // error[TK2349]: This expression is not callable
    new Wu6aReviewOuterNumber(); // error[TK2351]: This expression is not constructable
  }
}

const wu6aReviewTypeOnlyQualified: Wu6aReviewTypeOnlyHost.Wu6aReviewOuterNumber.Shape = {
  value: 1,
};
