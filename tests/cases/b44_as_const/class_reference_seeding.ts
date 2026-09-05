// Class publication seeds referenced shorthand values and direct enum-member operands
// before inferring the readonly const-context field surface.

declare const seededScalar: number;

enum SeededEnum {
  A,
}

class SeededConstFields {
  shorthand = ({ seededScalar }) as const;
  directEnum = SeededEnum.A as const;
  nestedEnum = ({ member: SeededEnum.A }) as const;
}

declare const seededFields: SeededConstFields;

const seededScalarType: number = seededFields.shorthand.seededScalar;
const seededDirectEnumType: SeededEnum.A = seededFields.directEnum;
const seededNestedEnumType: SeededEnum.A = seededFields.nestedEnum.member;

seededFields.shorthand.seededScalar = 1; // error[TK2540]: Cannot assign to 'seededScalar' because it is a read-only property
seededFields.nestedEnum.member = SeededEnum.A; // error[TK2540]: Cannot assign to 'member' because it is a read-only property
