// WU6A adversarial oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// A consumed standalone fragment still delegates every executable statement container through the
// ordinary checker in its fragment-private scope; publication never suppresses nested body errors.

namespace Wu6aReviewBodyTraversal {
  export const ready: number = 1;

  if (true) {
    const badIf: string = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
  }

  {
    missingFromBlock; // error[TK2304]: Cannot find name 'missingFromBlock'
  }

  for (let index = 0; index < 1; index++) {
    const badLoop: boolean = 1; // error[TK2322]: Type 'number' is not assignable to type 'boolean'
  }

  switch (ready) {
    case 1: {
      const badSwitch: string = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
      break;
    }
  }

  try {
    const badTry: number = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
  } finally {
    const badFinally: boolean = 1; // error[TK2322]: Type 'number' is not assignable to type 'boolean'
  }

  (() => {
    const badExpression: string = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
  })();
}

const wu6aReviewBodyReady: number = Wu6aReviewBodyTraversal.ready;
