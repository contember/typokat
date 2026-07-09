// backlog 65 - constructor inference shares the call-site candidate policy.

class SameBox<T> {
  first: T;
  second: T;
  constructor(first: T, second: T) {
    this.first = first;
    this.second = second;
  }
}

new SameBox(1, "s"); // error[TK2345]
const sameBoxOk = new SameBox(1, 2);
const sameBoxNumber: number = sameBoxOk.first;

interface HasX { x: number; }
declare const hasOnlyY: { y: number };
declare function constrainedSame<T extends HasX>(a: T, b: T): void;

constrainedSame({ x: 1 }, hasOnlyY); // error[TK2345]
constrainedSame(hasOnlyY, { x: 1 }); // error[TK2345]

declare function sameShape<T>(a: T, b: T): void;
sameShape({ x: 1 }, { x: 2 });
