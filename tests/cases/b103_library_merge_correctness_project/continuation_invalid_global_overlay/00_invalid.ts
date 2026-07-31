export {};

global { // error[TK2670]: Augmentations for the global scope should have 'declare' modifier unless they appear in already ambient context
  interface Array<T> {
    b103InvalidArrayLeak(): T;
  }

  interface B103InvalidFreshContinuation {
    value: number;
  }
}
