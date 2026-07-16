// Disabled WU6A second-review oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5
// --module commonjs. Attached namespace payloads retain direct const readonly metadata while
// let remains mutable for both function and class owners.

function Wu6aReviewConstFunction(): void {}
namespace Wu6aReviewConstFunction {
  export const fixed: number = 1;
  export let mutable: number = 1;
}
Wu6aReviewConstFunction.fixed = 2; // error[TK2540]: Cannot assign to 'fixed' because it is a read-only property
Wu6aReviewConstFunction.mutable = 2;

class Wu6aReviewConstClass {}
namespace Wu6aReviewConstClass {
  export const fixed: number = 1;
  export let mutable: number = 1;
}
Wu6aReviewConstClass.fixed = 2; // error[TK2540]: Cannot assign to 'fixed' because it is a read-only property
Wu6aReviewConstClass.mutable = 2;
