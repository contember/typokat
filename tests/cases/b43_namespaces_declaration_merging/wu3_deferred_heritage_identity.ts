// tsc 6.0.3 --strict --lib es5 --noEmit: alpha-equivalent generic method
// returns are identical through deferred semantic tags. Each neighboring unequal
// control owns one TS2320 at its derived interface binding.

interface ConditionalEqualLeft { f<T>(): T extends any ? T : never }
interface ConditionalEqualRight { f<U>(): U extends any ? U : never }
interface ConditionalEqual extends ConditionalEqualLeft, ConditionalEqualRight {}
interface ConditionalUnequalLeft { f<T>(): T extends any ? string : never }
interface ConditionalUnequalRight { f<U>(): U extends any ? number : never }
interface ConditionalUnequal extends ConditionalUnequalLeft, ConditionalUnequalRight {} // error[TK2320]: cannot simultaneously extend types 'ConditionalUnequalLeft' and 'ConditionalUnequalRight'

interface KeyofEqualLeft { f<T>(): keyof T }
interface KeyofEqualRight { f<U>(): keyof U }
interface KeyofEqual extends KeyofEqualLeft, KeyofEqualRight {}
interface KeyofUnequalLeft { f<T>(): keyof T }
interface KeyofUnequalRight { f<U>(): keyof U[] }
interface KeyofUnequal extends KeyofUnequalLeft, KeyofUnequalRight {} // error[TK2320]: cannot simultaneously extend types 'KeyofUnequalLeft' and 'KeyofUnequalRight'

interface TemplateEqualLeft { f<T extends string>(): `x${T}` }
interface TemplateEqualRight { f<U extends string>(): `x${U}` }
interface TemplateEqual extends TemplateEqualLeft, TemplateEqualRight {}
interface TemplateUnequalLeft { f<T extends string>(): `x${T}` }
interface TemplateUnequalRight { f<U extends string>(): `y${U}` }
interface TemplateUnequal extends TemplateUnequalLeft, TemplateUnequalRight {} // error[TK2320]: cannot simultaneously extend types 'TemplateUnequalLeft' and 'TemplateUnequalRight'

interface MappedEqualLeft { f<T>(): { [K in keyof T]: T[K] } }
interface MappedEqualRight { f<U>(): { [P in keyof U]: U[P] } }
interface MappedEqual extends MappedEqualLeft, MappedEqualRight {}
interface MappedUnequalLeft { f<T>(): { readonly [K in keyof T]: T[K] } }
interface MappedUnequalRight { f<U>(): { [P in keyof U]: U[P] } }
interface MappedUnequal extends MappedUnequalLeft, MappedUnequalRight {} // error[TK2320]: cannot simultaneously extend types 'MappedUnequalLeft' and 'MappedUnequalRight'

type DeferredBox<T> = T extends any ? { value: T } : never;
interface InstantiationEqualLeft { f<T>(): DeferredBox<T> }
interface InstantiationEqualRight { f<U>(): DeferredBox<U> }
interface InstantiationEqual extends InstantiationEqualLeft, InstantiationEqualRight {}
interface InstantiationUnequalLeft { f<T>(): DeferredBox<T> }
interface InstantiationUnequalRight { f<U>(): DeferredBox<U[]> }
interface InstantiationUnequal extends InstantiationUnequalLeft, InstantiationUnequalRight {} // error[TK2320]: cannot simultaneously extend types 'InstantiationUnequalLeft' and 'InstantiationUnequalRight'

interface IndexedEqualLeft { f<T extends { value: string; other: number }>(): T["value"] }
interface IndexedEqualRight { f<U extends { value: string; other: number }>(): U["value"] }
interface IndexedEqual extends IndexedEqualLeft, IndexedEqualRight {}
interface IndexedUnequalLeft { f<T extends { value: string; other: number }>(): T["value"] }
interface IndexedUnequalRight { f<U extends { value: string; other: number }>(): U["other"] }
interface IndexedUnequal extends IndexedUnequalLeft, IndexedUnequalRight {} // error[TK2320]: cannot simultaneously extend types 'IndexedUnequalLeft' and 'IndexedUnequalRight'

// Normalization collapses a deferred duplicate intersection member before identity.
type DeferredIdentity<T> = T extends any ? T : never;
interface IntersectionCollapsedLeft { f<T>(): string & DeferredIdentity<string> }
interface IntersectionCollapsedRight { f<U>(): string }
interface IntersectionCollapsedEqual extends IntersectionCollapsedLeft, IntersectionCollapsedRight {}
interface IntersectionUnequalLeft { f<T>(): string & DeferredIdentity<string> }
interface IntersectionUnequalRight { f<U>(): number }
interface IntersectionUnequal extends IntersectionUnequalLeft, IntersectionUnequalRight {} // error[TK2320]: cannot simultaneously extend types 'IntersectionUnequalLeft' and 'IntersectionUnequalRight'

// One diagnostic is coalesced only after the first semantic failure. The raw-different
// `a` properties normalize equal; the later canonical `b` comparison must still report.
interface PairSuppressionLeft { a: DeferredIdentity<string>; b: number }
interface PairSuppressionRight { a: string; b: string }
interface PairSuppressionConflict extends PairSuppressionLeft, PairSuppressionRight {} // error[TK2320]: cannot simultaneously extend types 'PairSuppressionLeft' and 'PairSuppressionRight'

// Reversed source member order preserves the same canonical first-failed result.
interface PairSuppressionReverseLeft { b: number; a: DeferredIdentity<string> }
interface PairSuppressionReverseRight { b: string; a: string }
interface PairSuppressionReverseConflict extends PairSuppressionReverseLeft, PairSuppressionReverseRight {} // error[TK2320]: cannot simultaneously extend types 'PairSuppressionReverseLeft' and 'PairSuppressionReverseRight'

// Public class declaring origins do not make equal projected fields nominal.
class PublicOriginLeft { value!: string }
class PublicOriginRight { value!: string }
interface PublicOriginBaseLeft { item: PublicOriginLeft }
interface PublicOriginBaseRight { item: PublicOriginRight }
interface PublicOriginEqual extends PublicOriginBaseLeft, PublicOriginBaseRight {}

// Private/protected declaring origins remain identity-bearing after projection.
class PrivateOriginLeft { private value!: string }
class PrivateOriginRight { private value!: string }
interface PrivateOriginBaseLeft { item: PrivateOriginLeft }
interface PrivateOriginBaseRight { item: PrivateOriginRight }
interface PrivateOriginUnequal extends PrivateOriginBaseLeft, PrivateOriginBaseRight {} // error[TK2320]: cannot simultaneously extend types 'PrivateOriginBaseLeft' and 'PrivateOriginBaseRight'

class ProtectedOriginLeft { protected value!: string }
class ProtectedOriginRight { protected value!: string }
interface ProtectedOriginBaseLeft { item: ProtectedOriginLeft }
interface ProtectedOriginBaseRight { item: ProtectedOriginRight }
interface ProtectedOriginUnequal extends ProtectedOriginBaseLeft, ProtectedOriginBaseRight {} // error[TK2320]: cannot simultaneously extend types 'ProtectedOriginBaseLeft' and 'ProtectedOriginBaseRight'

// Property identity includes supported modifiers, not only the read TypeId.
interface ReadonlyMetadataLeft { readonly value: string }
interface ReadonlyMetadataRight { value: string }
interface ReadonlyMetadataUnequal extends ReadonlyMetadataLeft, ReadonlyMetadataRight {} // error[TK2320]: cannot simultaneously extend types 'ReadonlyMetadataLeft' and 'ReadonlyMetadataRight'

interface OptionalMetadataLeft { value?: string }
interface OptionalMetadataRight { value: string | undefined }
interface OptionalMetadataUnequal extends OptionalMetadataLeft, OptionalMetadataRight {} // error[TK2320]: cannot simultaneously extend types 'OptionalMetadataLeft' and 'OptionalMetadataRight'
