// tsc 6.0.3 --strict --target es2025: TS2559 x11, TS2322 x12, and TS2741 x5 below.
// typokat uses TK2322 for the deferred weak-target diagnostic family (backlog 75).

interface B14WeakNativeTarget {
  b14WeakOnly?: number;
}

declare const b14WeakNumberSource: number;
declare const b14WeakBooleanSource: boolean;
declare const b14WeakFunctionSource: () => void;
declare const b14WeakArraySource: Array<number>;
declare const b14WeakTupleSource: [number, string];
declare const b14WeakReadonlyArraySource: ReadonlyArray<number>;
declare const b14WeakReadonlyTupleSource: readonly [number, string];
declare const b14WeakStringSource: string;
declare const b14WeakTemplateSource: `x-${string}`;
declare const b14WeakObjectSource: object;
declare const b14WeakStringOrNumberSource: string | number;
declare const b14WeakStringLiteralSource: "literal";
declare const b14WeakNumberLiteralSource: 1;
declare const b14WeakBooleanLiteralSource: true;

const b14WeakFromNumber: B14WeakNativeTarget = b14WeakNumberSource; // error[TK2322]
const b14WeakFromBoolean: B14WeakNativeTarget = b14WeakBooleanSource; // error[TK2322]
const b14WeakFromFunction: B14WeakNativeTarget = b14WeakFunctionSource; // error[TK2322]
const b14WeakFromArray: B14WeakNativeTarget = b14WeakArraySource; // error[TK2322]
const b14WeakFromTuple: B14WeakNativeTarget = b14WeakTupleSource; // error[TK2322]
const b14WeakFromReadonlyArray: B14WeakNativeTarget = b14WeakReadonlyArraySource; // error[TK2322]
const b14WeakFromReadonlyTuple: B14WeakNativeTarget = b14WeakReadonlyTupleSource; // error[TK2322]
const b14WeakFromString: B14WeakNativeTarget = b14WeakStringSource; // error[TK2322]
const b14WeakFromTemplate: B14WeakNativeTarget = b14WeakTemplateSource; // error[TK2322]
const b14WeakFromStringLiteral: B14WeakNativeTarget = b14WeakStringLiteralSource; // error[TK2322]

// A non-primitive `object` and the structural empty target do not trigger the weak-type rule.
const b14WeakFromObject: B14WeakNativeTarget = b14WeakObjectSource;
const b14EmptyTargetFromTemplate: {} = b14WeakTemplateSource;

const b14StringRequiredLength: { length: number } = b14WeakStringSource;
const b14TemplateRequiredLength: { length: number } = b14WeakTemplateSource;
const b14FunctionRequiredLength: { length: number } = b14WeakFunctionSource;

interface B14OptionalLengthTarget {
  length?: number;
}

interface B14WrongOptionalLengthTarget {
  length?: string;
}

const b14WeakCommonPropertyClean: B14OptionalLengthTarget = b14WeakStringSource;
const b14WeakTemplateCommonPropertyClean: B14OptionalLengthTarget = b14WeakTemplateSource;
const b14WeakArrayCommonPropertyClean: B14OptionalLengthTarget = b14WeakArraySource;
const b14WeakTupleCommonPropertyClean: B14OptionalLengthTarget = b14WeakTupleSource;
const b14WeakReadonlyArrayCommonPropertyClean: B14OptionalLengthTarget =
  b14WeakReadonlyArraySource;
const b14WeakReadonlyTupleCommonPropertyClean: B14OptionalLengthTarget =
  b14WeakReadonlyTupleSource;
// Function's apparent `length` satisfies a required target but does not count for weak overlap.
const b14WeakFunctionOptionalLength: B14OptionalLengthTarget = b14WeakFunctionSource; // error[TK2322]
const b14WeakUnionOptionalLength: B14OptionalLengthTarget = b14WeakStringOrNumberSource; // error[TK2322]
const b14WeakCommonPropertyMismatch: B14WrongOptionalLengthTarget = b14WeakStringSource; // error[TK2322]

const b14WeakNumberValueOfClean: { valueOf?: () => number } = b14WeakNumberSource;
const b14WeakNumberValueOfMismatch: { valueOf?: () => string } = b14WeakNumberSource; // error[TK2322]
const b14WeakBooleanValueOfClean: { valueOf?: () => boolean } = b14WeakBooleanSource;
const b14WeakBooleanValueOfMismatch: { valueOf?: () => string } = b14WeakBooleanSource; // error[TK2322]

interface B14RequiredMissingTarget {
  b14RequiredMissing: string;
}

// Primitive, function, template, and literal roots use the generic assignment code.
const b14RequiredMissingFromNumber: B14RequiredMissingTarget = b14WeakNumberSource; // error[TK2322]
const b14RequiredMissingFromBoolean: B14RequiredMissingTarget = b14WeakBooleanSource; // error[TK2322]
const b14RequiredMissingFromFunction: B14RequiredMissingTarget = b14WeakFunctionSource; // error[TK2322]
const b14RequiredMissingFromString: B14RequiredMissingTarget = b14WeakStringSource; // error[TK2322]
const b14RequiredMissingFromTemplate: B14RequiredMissingTarget = b14WeakTemplateSource; // error[TK2322]
const b14RequiredMissingFromStringLiteral: B14RequiredMissingTarget = b14WeakStringLiteralSource; // error[TK2322]
const b14RequiredMissingFromNumberLiteral: B14RequiredMissingTarget = b14WeakNumberLiteralSource; // error[TK2322]
const b14RequiredMissingFromBooleanLiteral: B14RequiredMissingTarget = b14WeakBooleanLiteralSource; // error[TK2322]

// Collection and ordinary object roots retain the missing-property code.
const b14RequiredMissingFromArray: B14RequiredMissingTarget = b14WeakArraySource; // error[TK2741]
const b14RequiredMissingFromTuple: B14RequiredMissingTarget = b14WeakTupleSource; // error[TK2741]
const b14RequiredMissingFromReadonlyArray: B14RequiredMissingTarget = b14WeakReadonlyArraySource; // error[TK2741]
const b14RequiredMissingFromReadonlyTuple: B14RequiredMissingTarget = b14WeakReadonlyTupleSource; // error[TK2741]
const b14RequiredMissingFromObject: B14RequiredMissingTarget = b14WeakObjectSource; // error[TK2741]
const b14ObjectToStringControl: { toString(): string } = b14WeakObjectSource;
