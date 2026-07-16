// WU6A negative oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// Exact result: TS2631 x4, TS2349, and TS2351.

namespace Wu6aRootOperations {
  export const value: number = 1;
}

Wu6aRootOperations = Wu6aRootOperations; // error[TK2631]: Cannot assign to 'Wu6aRootOperations' because it is a namespace
Wu6aRootOperations++; // error[TK2631]: Cannot assign to 'Wu6aRootOperations' because it is a namespace
++Wu6aRootOperations; // error[TK2631]: Cannot assign to 'Wu6aRootOperations' because it is a namespace
Wu6aRootOperations--; // error[TK2631]: Cannot assign to 'Wu6aRootOperations' because it is a namespace
Wu6aRootOperations(); // error[TK2349]: This expression is not callable
new Wu6aRootOperations(); // error[TK2351]: This expression is not constructable
