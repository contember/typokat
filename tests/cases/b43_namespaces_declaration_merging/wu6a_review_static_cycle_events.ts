// Initially disabled WU6A second-review oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5
// --module commonjs. Every genuine static-root cycle retains its exact owner; an ordinary function
// expression with a block-local same-name binding must not fabricate another cycle event.

namespace Wu6aReviewMultipleStaticCycles {
  export class First { // incomplete[decl/class-declaration/namespace-payload-static-cycle]
    static root = Wu6aReviewMultipleStaticCycles; // incomplete[class/property-definition/initializer-inference]
  }

  export class Second { // incomplete[decl/class-declaration/namespace-payload-static-cycle]
    static root = Wu6aReviewMultipleStaticCycles; // incomplete[class/property-definition/initializer-inference]
  }
}

const wu6aReviewMultipleUnavailable = Wu6aReviewMultipleStaticCycles;

namespace Wu6aReviewFunctionBlockShadow {
  export class Box {
    static project = function () { // incomplete[class/property-definition/initializer-inference]
      {
        const Wu6aReviewFunctionBlockShadow = 1;
        return Wu6aReviewFunctionBlockShadow;
      }
    };
  }
}
