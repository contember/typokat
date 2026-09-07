interface ConstraintUseBox<X, Y> { value: Y }
type ConstraintUseSecond<X, Y> = Y;
type ConstraintUseReverse<X, Y> = [Y, X];

declare function useDirectObject<A extends { value: B }, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const directUseResult = useDirectObject();
const directUseValid: { b: unknown } = directUseResult[1].c;
const directUseInvalid: { c: unknown } = directUseResult[2].b; // error[TK2322]

declare function useInterface<A extends ConstraintUseBox<C, B>, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const interfaceUseResult = useInterface();
const interfaceUseInvalid: { b: unknown } = interfaceUseResult[1].c; // error[TK2322]
const interfaceUseValid: { c: unknown } = interfaceUseResult[2].b;

declare function useUnionFirst<A extends C | B, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const unionFirstUseResult = useUnionFirst();
const unionFirstUseInvalid: { b: unknown } = unionFirstUseResult[1].c; // error[TK2322]
const unionFirstUseValid: { c: unknown } = unionFirstUseResult[2].b;

declare function useUnionSecond<A extends B | C, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const unionSecondUseResult = useUnionSecond();
const unionSecondUseValid: { b: unknown } = unionSecondUseResult[1].c;
const unionSecondUseInvalid: { c: unknown } = unionSecondUseResult[2].b; // error[TK2322]

declare function useErasedAlias<A extends ConstraintUseSecond<C, B>, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const erasedUseResult = useErasedAlias();
const erasedUseValid: { b: unknown } = erasedUseResult[1].c;
const erasedUseInvalid: { c: unknown } = erasedUseResult[2].b; // error[TK2322]

declare function useRetainedAlias<A extends ConstraintUseReverse<C, B>, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const retainedUseResult = useRetainedAlias();
const retainedUseInvalid: { b: unknown } = retainedUseResult[1].c; // error[TK2322]
const retainedUseValid: { c: unknown } = retainedUseResult[2].b;

declare function useIntersectionFirst<A extends C & B, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const intersectionFirstUseResult = useIntersectionFirst();
const intersectionFirstUseInvalid: { b: unknown } = intersectionFirstUseResult[1].c; // error[TK2322]
const intersectionFirstUseValid: { c: unknown } = intersectionFirstUseResult[2].b;

declare function useIntersectionSecond<A extends B & C, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const intersectionSecondUseResult = useIntersectionSecond();
const intersectionSecondUseValid: { b: unknown } = intersectionSecondUseResult[1].c;
const intersectionSecondUseInvalid: { c: unknown } = intersectionSecondUseResult[2].b; // error[TK2322]
