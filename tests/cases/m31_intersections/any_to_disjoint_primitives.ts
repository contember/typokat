// Backlog 107 - a provably disjoint primitive intersection has `never`
// semantics. In particular, `any` must not enter the uninhabited target.

declare const sourceAny: any;
declare const sourceUnknown: unknown;
declare const sourceNever: never;

type StringNumber = string & number;
type NumberString = number & string;
type StringBoolean = string & boolean;

const anyToStringNumber: StringNumber = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToNumberString: NumberString = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'
const anyToStringBoolean: StringBoolean = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'

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

// Disjoint literals of one primitive family share the same reduction.
type DisjointLiterals = "left" & "right";
const anyToDisjointLiterals: DisjointLiterals = sourceAny; // error[TK2322]: Type 'any' is not assignable to type 'never'

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
