// WU6A adversarial oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib esnext --module commonjs.
// Rejected forms receive their exact context diagnostic. A valid private using declaration can
// instantiate the namespace but contributes no public property.

namespace Wu6aReviewPrivateUsing {
  using hidden = null;
  export const visible: number = 1;
}

const wu6aReviewPrivateUsingRoot = Wu6aReviewPrivateUsing;
const wu6aReviewPrivateUsingVisible: number = Wu6aReviewPrivateUsing.visible;
Wu6aReviewPrivateUsing.hidden; // error[TK2339]: Property 'hidden' does not exist

namespace Wu6aReviewAwaitUsing {
  await using value = null; // error[TK2852]: 'await using' statements are only allowed within async functions and at the top levels of modules
}

namespace Wu6aReviewExportUsing {
  export using value = null; // error[TK1491]: 'export' modifier cannot appear on a 'using' declaration
}

namespace Wu6aReviewExportAwaitUsing {
  export await using value = null; // error[TK1495]: 'export' modifier cannot appear on an 'await using' declaration
}

declare namespace Wu6aReviewAmbientUsing {
  using value = null; // error[TK1545]: 'using' declarations are not allowed in ambient contexts | error[TK1254]: A 'const' initializer in an ambient context must be a string or numeric literal or literal enum reference
}
