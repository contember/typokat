// Disabled WU6A second-review oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5
// --module commonjs. The exact legal Function↔Namespace pair cannot exempt a third value row;
// typokat additionally owns the third row through the precise backlog-18 incomplete event.

namespace Wu6aReviewDuplicateLegality {
  export function Owner(): void {} // error[TK2300]: Duplicate identifier 'Owner'
  export namespace Owner { // error[TK2300]: Duplicate identifier 'Owner'
    export const tag: number = 1;
  }
  export const Owner: number = 1; // error[TK2300]: Duplicate identifier 'Owner' | incomplete[decl/variable-declaration/namespace-payload-duplicate-value]
}

const wu6aReviewDuplicateUnavailable = Wu6aReviewDuplicateLegality;
