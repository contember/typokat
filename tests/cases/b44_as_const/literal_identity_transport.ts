// The same displayed literal has different widening behavior across published boundaries.

const ordinaryIdentity = "same";
const constIdentityValue = "same" as const;
const annotatedIdentity: "same" = "same";
let ordinaryIdentityCopy = ordinaryIdentity;
ordinaryIdentityCopy = "different";
let constIdentityCopy = constIdentityValue;
constIdentityCopy = "different"; // error[TK2322]
let annotatedIdentityCopy = annotatedIdentity;
annotatedIdentityCopy = "different"; // error[TK2322]

class LiteralIdentityFields {
  readonly ordinary = "same";
  readonly asserted = "same" as const;
}

declare const literalIdentityFields: LiteralIdentityFields;
let ordinaryFieldCopy = literalIdentityFields.ordinary;
ordinaryFieldCopy = "different";
let assertedFieldCopy = literalIdentityFields.asserted;
assertedFieldCopy = "different"; // error[TK2322]
