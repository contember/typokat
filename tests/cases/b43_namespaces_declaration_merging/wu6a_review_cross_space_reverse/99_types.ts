// WU6A adversarial project oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5
// --module commonjs 00_values.ts 99_types.ts. Reversing source/input order preserves the same root.

interface Wu6aReviewCrossInterface {
  interfaceSide: number;
}

type Wu6aReviewCrossAlias = {
  aliasSide: string;
};

const wu6aReviewReverseInterfaceType: Wu6aReviewCrossInterface = { interfaceSide: 1 };
const wu6aReviewReverseAliasType: Wu6aReviewCrossAlias = { aliasSide: "alias" };
const wu6aReviewReverseInterfaceValue: number = Wu6aReviewCrossInterface.value;
const wu6aReviewReverseAliasValue: string = Wu6aReviewCrossAlias.value;
const wu6aReviewReverseInterfaceWrong: string = Wu6aReviewCrossInterface.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu6aReviewReverseAliasWrong: number = Wu6aReviewCrossAlias.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
