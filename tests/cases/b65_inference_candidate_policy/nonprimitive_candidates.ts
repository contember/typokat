// backlog 65 - non-primitive typed candidates must not be unioned to hide
// incompatible same-parameter arguments.

declare function sameObjects<T>(a: T, b: T): void;
declare const objectX: { x: number };
declare const objectY: { y: number };
sameObjects(objectX, objectY); // error[TK2345]

declare function sameArrays<T>(a: T, b: T): void;
declare const numbers: number[];
declare const strings: string[];
sameArrays(numbers, strings); // error[TK2345]

declare function sameFunctions<T>(a: T, b: T): void;
declare const numberFn: (x: number) => number;
declare const stringFn: (x: string) => string;
sameFunctions(numberFn, stringFn); // error[TK2345]

class SameBox<T> {
  constructor(first: T, second: T) {}
}

new SameBox(numberFn, stringFn); // error[TK2345]
