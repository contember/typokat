// Published ordinary and exact literals must remain distinct before shared inference.

namespace PublishedLiterals {
  export const ordinary = "same";
  export const exact: "same" = "same";
}

class PublishedLiteralFields {
  readonly ordinary = "same";
  readonly exact: "same" = "same";
  readonly asserted = "same" as const;
}

declare function publishedIdentity<T>(value: T): T;
declare const publishedFields: PublishedLiteralFields;

let ordinaryNamespaceCopy = publishedIdentity(PublishedLiterals.ordinary);
ordinaryNamespaceCopy = "different";
let exactNamespaceCopy = publishedIdentity(PublishedLiterals.exact);
exactNamespaceCopy = "different"; // error[TK2322]

let ordinaryPublishedFieldCopy = publishedIdentity(publishedFields.ordinary);
ordinaryPublishedFieldCopy = "different";
let exactPublishedFieldCopy = publishedIdentity(publishedFields.exact);
exactPublishedFieldCopy = "different"; // error[TK2322]
let assertedPublishedFieldCopy = publishedIdentity(publishedFields.asserted);
assertedPublishedFieldCopy = "different"; // error[TK2322]
