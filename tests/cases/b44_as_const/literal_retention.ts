// Backlog 44 `as const`: primitive, negative, enum, and parenthesized operands
// retain their exact value types. Ordinary expressions keep their existing widening.

enum ConstDirection {
  Up,
  Down,
}

const constString = "fixed" as const;
const constNumber = 1 as const;
const constNegative = -1 as const;
const constBoolean = true as const;
const constEnum = ConstDirection.Up as const;
const parenthesizedString = (("wrapped")) as const;
const parenthesizedNegative = ((-2)) as const;

const exactString: "fixed" = constString;
const exactNumber: 1 = constNumber;
const exactNegative: -1 = constNegative;
const exactBoolean: true = constBoolean;
const exactEnum: ConstDirection.Up = constEnum;
const exactParenthesizedString: "wrapped" = parenthesizedString;
const exactParenthesizedNegative: -2 = parenthesizedNegative;

const ordinaryObject = { text: "wide", count: 1 };
const ordinaryText: "wide" = ordinaryObject.text; // error[TK2322]
const ordinaryCount: 1 = ordinaryObject.count; // error[TK2322]

const ordinaryArray = ["wide", 1];
const ordinaryArrayText: "wide" = ordinaryArray[0]; // error[TK2322]
const ordinaryArrayNumber: 1 = ordinaryArray[1]; // error[TK2322]
