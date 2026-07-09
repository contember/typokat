// backlog 65 - fresh literals must not keep inference too broad when a typed
// structural candidate fixes T more narrowly, and nullish candidates can merge.

declare function same<T>(a: T, b: T): void;
declare const objectXY: { x: number; y: number };

same({ x: 1 }, objectXY); // error[TK2345]

class SameBox<T> {
  constructor(first: T, second: T) {}
}

new SameBox({ x: 1 }, objectXY); // error[TK2345]

declare function same3<T>(a: T, b: T, c: T): void;
same3(null, null, undefined);
same3(undefined, undefined, null);

declare function sameReturn<T>(a: T, b: T): T;
const nullString: null | string = sameReturn(null, "s");
const stringNull: null | string = sameReturn("s", null);

declare function constrainedNullish<T extends null | undefined | string>(a: T, b: T): T;
const constrainedNullString: null | string = constrainedNullish(null, "s");
const constrainedStringUndefined: string | undefined = constrainedNullish("s", undefined);
