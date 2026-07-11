// `${number}` has a broader lexical acceptance than Number::toString output.
// These controls prevent the construction fix from tightening the pattern matcher.

type NumericPattern = `${number}`;
const ordinaryInteger: NumericPattern = "42";
const ordinaryDecimal: NumericPattern = "0.5";
const longDecimal: NumericPattern = "1000000000000000000000";

type TaggedLarge = `value:${1e21}`;
const taggedLarge: TaggedLarge = "value:1e+21";
const taggedLargeWrong: TaggedLarge = "value:1000000000000000000000"; // error[TK2322]

type LargeOrSmall = `${1e21 | 5}`;
const unionLarge: LargeOrSmall = "1e+21";
const unionSmall: LargeOrSmall = "5";
const unionWrong: LargeOrSmall = "1000000000000000000000"; // error[TK2322]
