// Initially disabled WU6A second-review oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5
// --module commonjs. Namespace/private-callable alias dependencies must stage before root
// publication, while simple const provenance is known before an earlier function body is checked.

declare namespace Wu6aReviewAliasDependencies {
  namespace HiddenChild {
    const nestedValue: number;
  }
  export { HiddenChild as Child };

  function hiddenCall(value: number): number;
  export { hiddenCall as call };

  const hiddenFixed: number;
  export { hiddenFixed as fixed };
}

const wu6aReviewNestedAlias: number = Wu6aReviewAliasDependencies.Child.nestedValue;
const wu6aReviewCallableAlias: number = Wu6aReviewAliasDependencies.call(1);
// tsc deliberately treats an ambient const export alias as writable, unlike direct export const.
Wu6aReviewAliasDependencies.fixed = 2;

namespace Wu6aReviewLateRoot {
  export const value: number = 1;
}

function wu6aReviewUseLateAlias(): void {
  wu6aReviewLateAlias(); // error[TK2349]: This expression is not callable
  new wu6aReviewLateAlias(); // error[TK2351]: This expression is not constructable
}

const wu6aReviewLateAlias = Wu6aReviewLateRoot;
