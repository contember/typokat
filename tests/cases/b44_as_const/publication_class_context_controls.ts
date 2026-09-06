interface BroadPublicationKeys { [key: string]: number }
interface BroadPublicationHolder { key: keyof BroadPublicationKeys }
interface LiteralPublicationKeys { alpha: number; beta: string }

class ExistingClassContextControls {
  broad = { key: "alpha" } satisfies BroadPublicationHolder;
  readonly declaredKey: keyof LiteralPublicationKeys = "alpha";
  explicitReturn = (): keyof LiteralPublicationKeys => "alpha";
  exact = { key: "alpha" as const } satisfies { key: keyof LiteralPublicationKeys };
  unused = { value: 1 } satisfies { value: number; unused?: keyof LiteralPublicationKeys };
}

const existingClassContext = new ExistingClassContextControls();
let broadClassContextCopy = existingClassContext.broad.key;
broadClassContextCopy = "beta";
let declaredClassKeyCopy = existingClassContext.declaredKey;
declaredClassKeyCopy = "beta";
let explicitClassReturnCopy = existingClassContext.explicitReturn();
explicitClassReturnCopy = "beta";
const exactClassContextCopy: "alpha" = existingClassContext.exact.key;
