enum MemberNames {
  Identifier = 1,
  "quoted-name" = 2,
  ["computed-name"] = 3,
  [`template-name`] = 4,
  Negative = -1,
  Parenthesized = (2),
  Previous = Identifier,
  QualifiedPrevious = MemberNames.Identifier,
  Arithmetic = Identifier + 4,
  Bitwise = Arithmetic << 1,
}

const identifier: MemberNames.Identifier = MemberNames.Identifier;
const quotedLiteral: 2 = MemberNames["quoted-name"];
const computedLiteral: 3 = MemberNames["computed-name"];
const templateLiteral: 4 = MemberNames["template-name"];
const previous: MemberNames.Previous = MemberNames.Identifier;
const qualifiedPrevious: 1 = MemberNames.QualifiedPrevious;
const arithmeticLiteral: 5 = MemberNames.Arithmetic;
const bitwiseLiteral: 10 = MemberNames.Bitwise;

const wrongQuoted: MemberNames.Identifier = MemberNames["quoted-name"]; // error[TK2322]
const wrongComputed: MemberNames.Identifier = MemberNames["computed-name"]; // error[TK2322]

enum BitwiseCoercions {
  Wrap = 4294967296 | 0,
  Unsigned = -1 >>> 0,
  Fraction = 1.9 | 0,
  NegativeFraction = -1.9 | 0,
  NanOr = (0 / 0) | 0,
  InfinityOr = (1 / 0) | 0,
  NotNan = ~(0 / 0),
}

const wrappedLiteral: 0 = BitwiseCoercions.Wrap;
const unsignedLiteral: 4294967295 = BitwiseCoercions.Unsigned;
const fractionLiteral: 1 = BitwiseCoercions.Fraction;
const negativeFractionLiteral: -1 = BitwiseCoercions.NegativeFraction;
const nanOrLiteral: 0 = BitwiseCoercions.NanOr;
const infinityOrLiteral: 0 = BitwiseCoercions.InfinityOr;
const notNanLiteral: -1 = BitwiseCoercions.NotNan;

const wrongWrapped: BitwiseCoercions.Wrap = 2147483647; // error[TK2322]
const wrongUnsigned: BitwiseCoercions.Unsigned = 0; // error[TK2322]
