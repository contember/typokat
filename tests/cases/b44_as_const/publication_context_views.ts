interface ContextViewKeys { alpha: number; beta: string }
type ContextViewKey = keyof ContextViewKeys;
type ContextViewHolder = { key: ContextViewKey };
type ContextViewMapped = { [P in "key"]: ContextViewKey };
declare const contextViewFlag: boolean;

const aliasContextView = { key: "alpha" } satisfies ContextViewHolder;
let aliasContextCopy = aliasContextView.key;
aliasContextCopy = "beta"; // error[TK2322]
const mappedContextView = { key: "beta" } satisfies ContextViewMapped;
let mappedContextCopy = mappedContextView.key;
mappedContextCopy = "alpha"; // error[TK2322]
const nestedContextView = { nested: { key: "alpha" } } satisfies { nested: ContextViewHolder };
let nestedContextCopy = nestedContextView.nested.key;
nestedContextCopy = "beta"; // error[TK2322]

const broadContextView = { key: "alpha" } satisfies { key: ContextViewKey | string };
let broadContextCopy = broadContextView.key;
broadContextCopy = "beta";
const reversedBroadContextView = { key: "alpha" } satisfies { key: string | ContextViewKey };
let reversedBroadContextCopy = reversedBroadContextView.key;
reversedBroadContextCopy = "beta";

const conditionalContextView = { key: (contextViewFlag ? "alpha" : "alpha") } satisfies ContextViewHolder;
let conditionalContextCopy = conditionalContextView.key;
conditionalContextCopy = "beta"; // error[TK2322]
const logicalContextView = { key: contextViewFlag && "alpha" } satisfies { key: false | ContextViewKey };
let logicalContextCopy = logicalContextView.key;
logicalContextCopy = "beta"; // error[TK2322]

const repeatedReturnContext = (() => {
  if (contextViewFlag) return "alpha";
  return "alpha";
}) satisfies () => ContextViewKey;
let repeatedReturnCopy = repeatedReturnContext();
repeatedReturnCopy = "beta"; // error[TK2322]

function preserveContextParameter<T extends "alpha" | "beta">(key: T) {
  const boxed = { key } satisfies { key: T };
  const same: T = boxed.key;
  const invalid: { key: T } = { key: "alpha" }; // error[TK2322]
}
