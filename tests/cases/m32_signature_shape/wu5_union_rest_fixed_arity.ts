// WU5 - a callable union inherits the stricter finite arity when one member
// has an unbounded rest tail and the other member is fixed-width.

type RestTail = (x: string, ...rest: number[]) => void;
type FixedTwo = (x: string, y: number) => void;

declare const restThenFixed: RestTail | FixedTwo;
restThenFixed("x", 1);
restThenFixed("x"); // error[TK2554]: Expected 2 arguments, but got 1
restThenFixed("x", 1, 2); // error[TK2554]: Expected 2 arguments, but got 3

declare const fixedThenRest: FixedTwo | RestTail;
fixedThenRest("x", 1);
fixedThenRest("x"); // error[TK2554]: Expected 2 arguments, but got 1
fixedThenRest("x", 1, 2); // error[TK2554]: Expected 2 arguments, but got 3

type RestNumberOrBoolean = (x: string, ...rest: (number | boolean)[]) => void;
type RestNumberOrString = (x: string, ...rest: (number | string)[]) => void;

declare const allRest: RestNumberOrBoolean | RestNumberOrString;
allRest("x");
allRest("x", 1, 2, 3);

type GenericRest = <T>(x: T, ...rest: number[]) => T;
type GenericFixedTwo = <U>(x: U, y: number) => U;

declare const genericRestThenFixed: GenericRest | GenericFixedTwo;
genericRestThenFixed("x"); // error[TK2555]: Expected at least 2 arguments, but got 1
genericRestThenFixed("x", 1);
genericRestThenFixed("x", 1, 2);

declare const genericFixedThenRest: GenericFixedTwo | GenericRest;
genericFixedThenRest("x"); // error[TK2555]: Expected at least 2 arguments, but got 1
genericFixedThenRest("x", 1);
genericFixedThenRest("x", 1, 2);

type GenericFixedThree = <V>(x: V, y: number, z: number) => V;

declare const genericTripleForward: GenericRest | GenericFixedTwo | GenericFixedThree;
genericTripleForward("x", 1); // error[TK2555]: Expected at least 3 arguments, but got 2
genericTripleForward("x", 1, 2);
genericTripleForward("x", 1, 2, 3);

declare const genericTripleReverse: GenericFixedThree | GenericFixedTwo | GenericRest;
genericTripleReverse("x", 1); // error[TK2555]: Expected at least 3 arguments, but got 2
genericTripleReverse("x", 1, 2);
genericTripleReverse("x", 1, 2, 3);
