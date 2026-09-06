interface PublicationKeys { alpha: number; beta: string }
interface PublicationKeyHolder { key: keyof PublicationKeys }
declare const effectiveContextFlag: boolean;

const validEffectiveContext: PublicationKeyHolder = { key: "alpha" };
const invalidEffectiveContext: PublicationKeyHolder = { key: "missing" }; // error[TK2322]
const exactEffectiveContext = { key: "alpha" } satisfies PublicationKeyHolder;
let exactEffectiveContextCopy = exactEffectiveContext.key;
exactEffectiveContextCopy = "beta"; // error[TK2322]

const effectiveContextArray = ["alpha"] satisfies (keyof PublicationKeys)[];
let effectiveContextArrayCopy = effectiveContextArray[0];
effectiveContextArrayCopy = "beta"; // error[TK2322]
const effectiveContextTuple = ["beta"] satisfies [keyof PublicationKeys];
let effectiveContextTupleCopy = effectiveContextTuple[0];
effectiveContextTupleCopy = "alpha"; // error[TK2322]

const effectiveContextExpression = (() => "alpha") satisfies () => keyof PublicationKeys;
let effectiveContextExpressionCopy = effectiveContextExpression();
effectiveContextExpressionCopy = "beta"; // error[TK2322]
const effectiveContextBlock = (() => { return "beta"; }) satisfies () => keyof PublicationKeys;
let effectiveContextBlockCopy = effectiveContextBlock();
effectiveContextBlockCopy = "alpha"; // error[TK2322]

const effectiveContextUnion = { key: "alpha" } satisfies { key: number | keyof PublicationKeys };
let effectiveContextUnionCopy = effectiveContextUnion.key;
effectiveContextUnionCopy = "beta"; // error[TK2322]
const effectiveContextConditional = { key: effectiveContextFlag ? "alpha" : "beta" } satisfies PublicationKeyHolder;
let effectiveContextConditionalCopy = effectiveContextConditional.key;
effectiveContextConditionalCopy = "missing"; // error[TK2322]

class EffectiveContextFields {
  valid: PublicationKeyHolder = { key: "alpha" };
  invalid: PublicationKeyHolder = { key: "missing" }; // error[TK2322]
}
