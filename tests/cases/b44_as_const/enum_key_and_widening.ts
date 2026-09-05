// Direct enum authentication and ordinary enum-member widening have different boundaries.

enum KeyedEnum {
  A,
  B,
}

const enumKey = "A";
const keyedEnumValue = KeyedEnum[enumKey] as const;
const exactKeyedEnumValue: KeyedEnum.A = keyedEnumValue;

const OrdinaryEnumAlias = KeyedEnum;
let wideningEnumAlias = OrdinaryEnumAlias.A;
wideningEnumAlias = KeyedEnum.B;

const invalidAliasedOperand = OrdinaryEnumAlias.A as const; // incomplete[expr-infer/as-const/invalid-operand]
