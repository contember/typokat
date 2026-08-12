enum Numeric {
  Zero,
  Two = 2,
  Three,
  Negative = -1,
  Derived = Two + 3,
}

enum OtherNumeric {
  Zero,
}

enum SameNumericValue {
  First = 1,
  Second = 1,
}

enum EmptyNumeric {}

enum OtherEmptyNumeric {}

declare function nextNumeric(): number;

enum OpaqueNumeric {
  Value = nextNumeric(),
}

declare let broadNumber: number;

const wholeFromMember: Numeric = Numeric.Two;
const memberFromSelf: Numeric.Two = Numeric.Two;
const memberToLiteral: 2 = Numeric.Two;
const knownRawToWhole: Numeric = 2;
const knownRawToMember: Numeric.Two = 2;
const broadToWhole: Numeric = broadNumber;
const broadToMember: Numeric.Two = broadNumber;
const enumToNumber: number = Numeric.Derived;
const wholeToLiteralUnion: 0 | 2 | 3 | -1 | 5 = wholeNumeric;
const equalNumericAlias: SameNumericValue.First = SameNumericValue.Second;
const emptyFromNumber: EmptyNumeric = 123;
const opaqueWholeFromNumber: OpaqueNumeric = 123;
const opaqueMemberFromNumber: OpaqueNumeric.Value = 123;

declare const wholeNumeric: Numeric;
declare const otherEmpty: OtherEmptyNumeric;

const unknownRawToWhole: Numeric = 99; // error[TK2322]
const wrongRawToMember: Numeric.Two = 3; // error[TK2322]
const wrongMember: Numeric.Two = Numeric.Three; // error[TK2322]
const crossEnum: Numeric = OtherNumeric.Zero; // error[TK2322]
const wholeToMember: Numeric.Two = wholeNumeric; // error[TK2322]
const crossEmpty: EmptyNumeric = otherEmpty; // error[TK2322]
const nullToEnum: Numeric = null; // error[TK2322]
const undefinedToEnum: Numeric = undefined; // error[TK2322]
