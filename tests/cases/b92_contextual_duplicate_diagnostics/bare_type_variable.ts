// backlog 92 - the bare-type-variable discriminator (real `zod`'s
// `object<T extends ZodRawShape>(shape: T)`).
//
// A parameter that IS a type variable never re-walks during candidate inference,
// so this shape already costs two walks per level rather than three - but BOTH of
// those walks retain effects, so the duplicate count is the same 2^depth. This is
// one of the two shapes that actually hang in the wild, so it must be pinned
// independently of the structurally-embedded generics above.
//
// tsc 6.0.3 --strict: exactly one TS2304 per line, at every depth.

declare function shapeOf<T>(shape: T): T;

const bare1 = shapeOf({ inner: undeclaredThing }); // error[TK2304]: Cannot find name 'undeclaredThing'
const bare2 = shapeOf({ inner: shapeOf({ inner: undeclaredThing }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const bare3 = shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: undeclaredThing }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const bare4 = shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: undeclaredThing }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const bare5 = shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: undeclaredThing }) }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const bare6 = shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: undeclaredThing }) }) }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const bare7 = shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: undeclaredThing }) }) }) }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const bare8 = shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: shapeOf({ inner: undeclaredThing }) }) }) }) }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
