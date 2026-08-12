declare function nextNumber(): number;

enum ComputedNumeric {
  Opaque = nextNumber(),
  Fixed = 4,
}

const opaqueWhole: ComputedNumeric = ComputedNumeric.Opaque;
const opaqueMember: ComputedNumeric.Opaque = ComputedNumeric.Opaque;
const opaqueNumber: number = ComputedNumeric.Opaque;

enum MissingAfterComputed {
  Opaque = nextNumber(),
  Missing, // error[TK1061]
}

enum MissingAfterString {
  Text = "text",
  Missing, // error[TK1061]
}

enum ComputedString {
  Bad = `${nextNumber()}`, // error[TK18033]
}

enum ComputedBoolean {
  Bad = true, // error[TK18033]
}

const enum ConstComputed {
  Bad = nextNumber(), // error[TK2474]
}

enum MissingName {
  Bad = missingInitializer, // error[TK2304]
}

enum ForwardReference {
  First = Later, // error[TK2651]
  Later = 1,
}

enum SelfReference {
  Member = Member, // error[TK2565]
}
