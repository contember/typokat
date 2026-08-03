// Backlog 107 - a provably disjoint primitive intersection has `never`
// semantics. In particular, `any` must not enter the uninhabited target.

declare const sourceAny: any;
declare const sourceUnknown: unknown;
declare const sourceNever: never;
declare const sourceObject: object;

type StringNumber = string & number;
type NumberString = number & string;
type StringBoolean = string & boolean;
type NumberBigint = number & bigint;
type StringSymbol = string & symbol;
type StringNull = string & null;
type StringUndefined = string & undefined;
type StringVoid = string & void;
type ObjectString = object & string;
type NullUndefined = null & undefined;

const anyToStringNumber: StringNumber = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToNumberString: NumberString = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToStringBoolean: StringBoolean = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToNumberBigint: NumberBigint = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToStringSymbol: StringSymbol = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToStringNull: StringNull = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToStringUndefined: StringUndefined = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToStringVoid: StringVoid = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToObjectString: ObjectString = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToNullUndefined: NullUndefined = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'

// Existing non-`any` rejections must not move while the target is normalized.
const stringToStringNumber: StringNumber = "x"; // error[TK2322]
const numberToStringNumber: StringNumber = 1; // error[TK2322]
const unknownToStringNumber: StringNumber = sourceUnknown; // error[TK2322]

// `never` flows into every target, and a reduced disjoint intersection flows
// back out as `never` and therefore into either original primitive.
const neverToStringNumber: StringNumber = sourceNever;
declare const reducedStringNumber: StringNumber;
const reducedToNever: never = reducedStringNumber;
const reducedToString: string = reducedStringNumber;
const reducedToNumber: number = reducedStringNumber;

declare const reducedNumberBigint: NumberBigint;
declare const reducedStringSymbol: StringSymbol;
declare const reducedStringNull: StringNull;
declare const reducedStringUndefined: StringUndefined;
declare const reducedStringVoid: StringVoid;
declare const reducedObjectString: ObjectString;
declare const reducedNullUndefined: NullUndefined;
const numberBigintToNever: never = reducedNumberBigint;
const stringSymbolToNever: never = reducedStringSymbol;
const stringNullToNever: never = reducedStringNull;
const stringUndefinedToNever: never = reducedStringUndefined;
const stringVoidToNever: never = reducedStringVoid;
const objectStringToNever: never = reducedObjectString;
const nullUndefinedToNever: never = reducedNullUndefined;

// Disjoint literals of one primitive family share the same reduction.
type DisjointLiterals = "left" & "right";
const anyToDisjointLiterals: DisjointLiterals = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'

// A finite union does not require general distribution when its primitive
// domain is structurally disjoint from the other member.
type UnionDisjoint = (string | number) & boolean;
const anyToUnionDisjoint: UnionDisjoint = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
declare const reducedUnionDisjoint: UnionDisjoint;
const unionDisjointToNever: never = reducedUnionDisjoint;

type LiteralUnionDisjoint = ("x" | "y") & "z";
const anyToLiteralUnionDisjoint: LiteralUnionDisjoint = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
declare const reducedLiteralUnionDisjoint: LiteralUnionDisjoint;
const literalUnionDisjointToNever: never = reducedLiteralUnionDisjoint;

type ObjectUnionDisjoint = (string | number) & object;
const anyToObjectUnionDisjoint: ObjectUnionDisjoint = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
declare const reducedObjectUnionDisjoint: ObjectUnionDisjoint;
const objectUnionDisjointToNever: never = reducedObjectUnionDisjoint;

// Overlap and branding remain inhabited; the reduction is not a blanket rule
// for every intersection containing a primitive.
type StringLiteralOverlap = string & "x";
const overlapFromLiteral: StringLiteralOverlap = "x";
const overlapFromAny: StringLiteralOverlap = sourceAny;
const overlapRejectsOtherLiteral: StringLiteralOverlap = "y"; // error[TK2322]

type BrandedString = string & { readonly __wu5Brand: "wu5" };
declare const brandedString: BrandedString;
const brandedFromAny: BrandedString = sourceAny;
const brandedAsString: string = brandedString;
const brandedTag: "wu5" = brandedString.__wu5Brand;

// Overlapping finite domains remain inhabited.
type UnionOverlap = (string | number) & string;
const unionOverlapFromString: UnionOverlap = "x";
const unionOverlapFromAny: UnionOverlap = sourceAny;
declare const unionOverlap: UnionOverlap;
const unionOverlapToString: string = unionOverlap;

type LiteralUnionOverlap = ("x" | "y") & "x";
const literalUnionOverlapFromX: LiteralUnionOverlap = "x";
const literalUnionOverlapFromAny: LiteralUnionOverlap = sourceAny;
declare const literalUnionOverlap: LiteralUnionOverlap;
const literalUnionOverlapToX: "x" = literalUnionOverlap;

type ObjectUnionOverlap = (object | string) & object;
const objectUnionOverlapFromObject: ObjectUnionOverlap = sourceObject;
const objectUnionOverlapFromAny: ObjectUnionOverlap = sourceAny;
declare const objectUnionOverlap: ObjectUnionOverlap;
const objectUnionOverlapToObject: object = objectUnionOverlap;

// `void & undefined` stays inhabited as `undefined`, and `0` and `-0`
// are one observable literal value.
type VoidUndefined = void & undefined;
const voidUndefinedFromUndefined: VoidUndefined = undefined;
const voidUndefinedFromAny: VoidUndefined = sourceAny;
declare const voidUndefined: VoidUndefined;
const voidUndefinedToUndefined: undefined = voidUndefined;

type ZeroNegativeZero = 0 & -0;
const zeroNegativeZeroFromZero: ZeroNegativeZero = 0;
const zeroNegativeZeroFromAny: ZeroNegativeZero = sourceAny;
declare const zeroNegativeZero: ZeroNegativeZero;
const zeroNegativeZeroToZero: 0 = zeroNegativeZero;
