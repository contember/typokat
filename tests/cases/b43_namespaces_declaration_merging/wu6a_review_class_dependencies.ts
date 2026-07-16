// WU6A adversarial oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// tsc reports no diagnostics. The incomplete marker is typokat's exact owner-76 terminal for the
// genuinely inference-dependent static/root cycle; it must withhold that complete namespace value.

namespace Wu6aReviewAnnotatedStatic {
  export class Box {
    static root: unknown = Wu6aReviewAnnotatedStatic;
  }
}

const wu6aReviewAnnotatedRoot = Wu6aReviewAnnotatedStatic;
const wu6aReviewAnnotatedBox = Wu6aReviewAnnotatedStatic.Box;

namespace Wu6aReviewShadowedStatic {
  export class Box {
    static project: (Wu6aReviewShadowedStatic: number) => number =
      (Wu6aReviewShadowedStatic: number): number => Wu6aReviewShadowedStatic;
  }
}

const wu6aReviewShadowedRoot = Wu6aReviewShadowedStatic;
const wu6aReviewShadowedResult: number = Wu6aReviewShadowedStatic.Box.project(1);

namespace Wu6aReviewTrueStaticCycle {
  export class Box { // incomplete[decl/class-declaration/namespace-payload-static-cycle]
    static root = Wu6aReviewTrueStaticCycle;
  }
}

const wu6aReviewUnavailableStaticRoot = Wu6aReviewTrueStaticCycle;
