// WU6A adversarial project oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5
// --module commonjs 00_types.ts 99_values.ts. Cross-file type/value companions share one root.

interface Wu6aReviewCrossInterface {
  interfaceSide: number;
}

type Wu6aReviewCrossAlias = {
  aliasSide: string;
};

const wu6aReviewForwardInterfaceType: Wu6aReviewCrossInterface = { interfaceSide: 1 };
const wu6aReviewForwardAliasType: Wu6aReviewCrossAlias = { aliasSide: "alias" };
const wu6aReviewForwardInterfaceValue: number = Wu6aReviewCrossInterface.value;
const wu6aReviewForwardAliasValue: string = Wu6aReviewCrossAlias.value;
const wu6aReviewForwardInterfaceWrong: string = Wu6aReviewCrossInterface.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu6aReviewForwardAliasWrong: number = Wu6aReviewCrossAlias.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
