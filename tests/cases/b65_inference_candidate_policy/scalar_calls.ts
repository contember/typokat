// backlog 65 - scalar call-site inference must fix T before replaying args.
// Cross-check with tsc 6.0.3 --strict before enabling this corpus.

declare function sameVoid<T>(a: T, b: T): void;
sameVoid(1, "s"); // error[TK2345]
sameVoid(1, 2);

declare function sameReturn<T>(a: T, b: T): T;
sameReturn(1, "s"); // error[TK2345]
const sameReturnOk: number = sameReturn(1, 2);

declare function sameConstrained<T extends string | number>(a: T, b: T): void;
sameConstrained(1, "s"); // error[TK2345]
sameConstrained(1, 2);

let widenedNumber: number = 1;
let widenedString: string = "s";
sameVoid(widenedNumber, widenedString); // error[TK2345]
sameVoid(widenedNumber, 2);

declare const numberOrString: number | string;
sameVoid(numberOrString, "s");
