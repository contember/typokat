const literalModeObject = { a: "a" } satisfies { a: "a" | number };
let literalModeObjectCopy = literalModeObject.a;
literalModeObjectCopy = "b"; // error[TK2322]
literalModeObjectCopy = "a";
literalModeObject.a = "b"; // error[TK2322]

const reversedModeObject = { a: "a" } satisfies { a: number | "a" };
let reversedModeCopy = reversedModeObject.a;
reversedModeCopy = "b"; // error[TK2322]
const optionalModeObject = { a: 1 } satisfies { a?: string | 1 };
let optionalModeCopy = optionalModeObject.a;
optionalModeCopy = 2; // error[TK2322]
const booleanModeObject = { a: true } satisfies { a: true | number };
let booleanModeCopy = booleanModeObject.a;
booleanModeCopy = false; // error[TK2322]

const literalModeArray = ["a"] satisfies ("a" | number)[];
let literalModeArrayCopy = literalModeArray[0];
literalModeArrayCopy = "b"; // error[TK2322]
literalModeArrayCopy = "a";
const literalModeTuple = [-1] satisfies [string | -1];
let literalModeTupleCopy = literalModeTuple[0];
literalModeTupleCopy = -2; // error[TK2322]
literalModeTupleCopy = -1;

const expressionModeReturn = (() => ({ a: "a" })) satisfies () => { a: "a" | number };
let expressionModeCopy = expressionModeReturn().a;
expressionModeCopy = "b"; // error[TK2322]
const blockModeReturn = (() => { return { a: "a" }; }) satisfies () => { a: "a" | number };
let blockModeCopy = blockModeReturn().a;
blockModeCopy = "b"; // error[TK2322]

const broadModeObject = { a: "a" } satisfies { a: "a" | string };
let broadModeObjectCopy = broadModeObject.a;
broadModeObjectCopy = "b";
const broadModeArray = [1] satisfies (1 | number)[];
let broadModeArrayCopy = broadModeArray[0];
broadModeArrayCopy = 2;
const broadModeTuple = ["a"] satisfies [string | "a"];
let broadModeTupleCopy = broadModeTuple[0];
broadModeTupleCopy = "b";
