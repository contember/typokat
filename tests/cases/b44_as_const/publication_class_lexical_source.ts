interface DeferredPublicationKeys { alpha: number; beta: string }
interface DeferredPublicationHolder { key: keyof DeferredPublicationKeys }
class ExternalContextOwner {
  member = { key: "alpha" } satisfies DeferredPublicationHolder; // error[TK1360]: Type 'string' does not satisfy
}
let externalContextCopy = new ExternalContextOwner().member.key;
externalContextCopy = "beta";
