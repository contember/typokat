declare const sourceUnionFlag: boolean;

const sourceUnionObject = { a: sourceUnionFlag ? true : undefined } satisfies { a: boolean | undefined };
let sourceUnionObjectCopy = sourceUnionObject.a;
sourceUnionObjectCopy = false; // error[TK2322]
sourceUnionObjectCopy = true;
sourceUnionObjectCopy = undefined;
const sourceUnionArray = [sourceUnionFlag ? true : undefined] satisfies (undefined | boolean)[];
let sourceUnionArrayCopy = sourceUnionArray[0];
sourceUnionArrayCopy = false; // error[TK2322]
const sourceUnionTuple = [sourceUnionFlag ? true : "a"] satisfies [boolean | string];
let sourceUnionTupleCopy = sourceUnionTuple[0];
sourceUnionTupleCopy = "b"; // error[TK2322]
sourceUnionTupleCopy = false; // error[TK2322]

const sourceUnionExpression = (() => ({ a: sourceUnionFlag ? true : undefined })) satisfies () => { a: boolean | undefined };
let sourceUnionExpressionCopy = sourceUnionExpression().a;
sourceUnionExpressionCopy = false; // error[TK2322]
const sourceUnionBlock = (() => { return { a: sourceUnionFlag ? true : undefined }; }) satisfies () => { a: undefined | boolean };
let sourceUnionBlockCopy = sourceUnionBlock().a;
sourceUnionBlockCopy = false; // error[TK2322]

const broadSourceUnionObject = { a: sourceUnionFlag ? "a" : undefined } satisfies { a: string | undefined };
let broadSourceUnionCopy = broadSourceUnionObject.a;
broadSourceUnionCopy = "b";
const broadNumericUnionObject = { a: sourceUnionFlag ? 1 : undefined } satisfies { a: undefined | number };
let broadNumericUnionCopy = broadNumericUnionObject.a;
broadNumericUnionCopy = 2;

const exactPrimitiveSource: "a" = "a";
const ordinaryPrimitiveSource = "a";
const exactPrimitiveContainer = { a: exactPrimitiveSource } satisfies { a: string };
let exactPrimitiveCopy = exactPrimitiveContainer.a;
exactPrimitiveCopy = "b"; // error[TK2322]
const ordinaryPrimitiveContainer = { a: ordinaryPrimitiveSource } satisfies { a: string };
let ordinaryPrimitiveCopy = ordinaryPrimitiveContainer.a;
ordinaryPrimitiveCopy = "b";

declare const publishedContainer: { a: string };
const referencedPublication = { nested: publishedContainer } as const;
referencedPublication.nested.a = "b";
referencedPublication.nested = publishedContainer; // error[TK2540]
