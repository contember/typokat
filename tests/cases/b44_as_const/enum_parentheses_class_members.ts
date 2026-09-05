// Enum authentication peels transparent parentheses. Member references nested inside a
// const-owned class-field literal remain ordinary reference-boundary expressions.

enum ParenthesizedEnum {
  A,
}

const parenthesizedEnum = ((ParenthesizedEnum)).A as const;
const parenthesizedComputedEnum = ((ParenthesizedEnum))["A"] as const;
const exactParenthesizedEnum: ParenthesizedEnum.A = parenthesizedEnum;
const exactParenthesizedComputedEnum: ParenthesizedEnum.A = parenthesizedComputedEnum;

declare const memberSource: { value: number };

class ConstMemberReferences {
  field = ({ value: memberSource.value }) as const;
  nested = ({ child: { value: memberSource.value } }) as const;
}

declare const memberReferences: ConstMemberReferences;

const classMemberValue: number = memberReferences.field.value;
const nestedClassMemberValue: number = memberReferences.nested.child.value;

memberReferences.field.value = 1; // error[TK2540]: Cannot assign to 'value' because it is a read-only property
memberReferences.nested.child.value = 1; // error[TK2540]: Cannot assign to 'value' because it is a read-only property
