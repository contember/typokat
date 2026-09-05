// Only direct enum-member references are valid enum const-assertion operands.

enum DirectEnum {
  A,
}

const directEnumMember = DirectEnum.A as const;
const computedDirectEnumMember = DirectEnum["A"] as const;
const exactDirectEnumMember: DirectEnum.A = directEnumMember;
const exactComputedEnumMember: DirectEnum.A = computedDirectEnumMember;

const DirectEnumAlias = DirectEnum;
const aliasedEnumMember = DirectEnumAlias.A as const; // incomplete[expr-infer/as-const/invalid-operand]

const enumCarrier = { member: DirectEnum.A };
const carriedEnumMember = enumCarrier.member as const; // incomplete[expr-infer/as-const/invalid-operand]

const missingDirectEnumMember = DirectEnum.Missing as const; // error[TK2339]
