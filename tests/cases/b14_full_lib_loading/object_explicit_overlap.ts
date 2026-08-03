// tsc 6.0.3 --strict --target es2025: TS2322 x18 and TS2696 x2 below.
// Object stays top-like when names are absent, but explicit overlaps remain structural.

interface B14WrongObjectInterface {
  toString(): number;
}

declare const b14WrongObjectInterface: B14WrongObjectInterface;
const b14ObjectFromWrongInterface: Object = b14WrongObjectInterface; // error[TK2322]

interface B14VoidObjectInterface {
  toString(): void;
}

declare const b14VoidObjectInterface: B14VoidObjectInterface;
const b14ObjectFromVoidInterface: Object = b14VoidObjectInterface; // error[TK2322]

class B14WrongObjectClass {
  toString(): number {
    return 1;
  }
}

declare const b14WrongObjectClass: B14WrongObjectClass;
const b14ObjectFromWrongClass: Object = b14WrongObjectClass; // error[TK2322]

const b14WrongObjectLiteral = { toString: () => 1 };
const b14ObjectFromWrongLiteral: Object = b14WrongObjectLiteral; // error[TK2322]

interface B14CompatibleObjectInterface {
  toString(): string;
}

interface B14ObjectWithoutOverlap {
  value: number;
}

declare const b14CompatibleObjectInterface: B14CompatibleObjectInterface;
declare const b14ObjectWithoutOverlap: B14ObjectWithoutOverlap;
const b14ObjectFromCompatibleInterface: Object = b14CompatibleObjectInterface;
const b14ObjectFromAbsentOverlap: Object = b14ObjectWithoutOverlap;

interface Object {
  b14OptionalObjectMember?: string;
  b14VisibilityMember?: number;
  length?: string;
}

interface B14AugmentedObjectMatch {
  b14OptionalObjectMember: string;
}

interface B14AugmentedObjectConflict {
  b14OptionalObjectMember: number;
}

interface B14AugmentedObjectAbsent {
  separate: boolean;
}

declare const b14AugmentedObjectMatch: B14AugmentedObjectMatch;
declare const b14AugmentedObjectConflict: B14AugmentedObjectConflict;
declare const b14AugmentedObjectAbsent: B14AugmentedObjectAbsent;
const b14ObjectFromAugmentedMatch: Object = b14AugmentedObjectMatch;
const b14ObjectFromAugmentedConflict: Object = b14AugmentedObjectConflict; // error[TK2322]
const b14ObjectFromAugmentedAbsent: Object = b14AugmentedObjectAbsent;

class B14PrivateObjectOverlap {
  private b14VisibilityMember = 1;
}

class B14ProtectedObjectOverlap {
  protected b14VisibilityMember = 1;
}

class B14PublicObjectOverlap {
  b14VisibilityMember = 1;
}

declare const b14PrivateObjectOverlap: B14PrivateObjectOverlap;
declare const b14ProtectedObjectOverlap: B14ProtectedObjectOverlap;
declare const b14PublicObjectOverlap: B14PublicObjectOverlap;
const b14ObjectFromPrivateOverlap: Object = b14PrivateObjectOverlap; // error[TK2322]
const b14ObjectFromProtectedOverlap: Object = b14ProtectedObjectOverlap; // error[TK2322]
const b14ObjectFromPublicOverlap: Object = b14PublicObjectOverlap;

declare const b14FunctionSource: () => void;
declare const b14ArraySource: number[];
declare const b14StringSource: string;
declare const b14TemplateSource: `prefix-${string}`;
declare const b14TupleSource: [number, number];
declare const b14ReadonlyArraySource: readonly number[];
declare const b14ReadonlyTupleSource: readonly [number, number];
declare const b14NeverSource: never;
const b14ObjectFromString: Object = "value"; // error[TK2322]
const b14ObjectFromStringIntrinsic: Object = b14StringSource; // error[TK2322]
const b14ObjectFromTemplate: Object = b14TemplateSource; // error[TK2322]
// Number and boolean retain compatible apparent Object members and no `length` overlap.
const b14ObjectFromNumber: Object = 1;
const b14ObjectFromBoolean: Object = true;
const b14ObjectFromFunction: Object = b14FunctionSource; // error[TK2322]
const b14ObjectFromArray: Object = b14ArraySource; // error[TK2322]
const b14ObjectFromTuple: Object = b14TupleSource; // error[TK2322]
const b14ObjectFromReadonlyArray: Object = b14ReadonlyArraySource; // error[TK2322]
const b14ObjectFromReadonlyTuple: Object = b14ReadonlyTupleSource; // error[TK2322]
const b14ObjectFromNever: Object = b14NeverSource;

declare const b14UnknownSource: unknown;
const b14ObjectFromNull: Object = null; // error[TK2322]
const b14ObjectFromUndefined: Object = undefined; // error[TK2322]
const b14ObjectFromUnknown: Object = b14UnknownSource; // error[TK2322]

declare const b14LibraryObject: Object;
const b14WrongInterfaceFromObject: B14WrongObjectInterface = b14LibraryObject; // error[TK2322]
const b14VoidInterfaceFromObject: B14VoidObjectInterface = b14LibraryObject;
const b14WrongClassFromObject: B14WrongObjectClass = b14LibraryObject; // error[TK2322]
