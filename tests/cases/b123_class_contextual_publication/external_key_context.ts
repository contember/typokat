interface DeferredPublicationKeys { alpha: number; beta: string }
interface DeferredPublicationHolder { key: keyof DeferredPublicationKeys }
class ExternalContextOwner {
  member = { key: "alpha" } satisfies DeferredPublicationHolder;
}
let externalContextCopy = new ExternalContextOwner().member.key;
externalContextCopy = "beta"; // error[TK2322]
