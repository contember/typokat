// tsc 6.0.3 --strict: clean; ambient namespace members are exported by default.
declare namespace AmbientRoot {
  interface Item<T> { value: T }
  namespace Nested {
    interface Leaf { leaf: true }
  }
}

declare namespace AmbientRoot {
  interface Reopened { reopened: true }
}

let ambientItem: AmbientRoot.Item<number>;
let ambientLeaf: AmbientRoot.Nested.Leaf;
let ambientReopened: AmbientRoot.Reopened;
