// Backlog 70 — the lib.es5 helper aliases consume the represented receiver slot;
// overload utilities preserve the last-signature rule.

type ExtractedReceiver = ThisParameterType<
  (this: { tag: "receiver" }, value: number) => string
>;

const receiverOk: ExtractedReceiver = { tag: "receiver" };
const receiverBad: ExtractedReceiver = { tag: "wrong" }; // error[TK2322]

type NoReceiver = ThisParameterType<(value: number) => string>;
const unknownReceiver: NoReceiver = 1;

type Omitted = OmitThisParameter<
  (this: { tag: "receiver" }, value: number) => string
>;

declare const omitted: Omitted;
const omittedOk: string = omitted(1);
omitted("wrong"); // error[TK2345]

type OptionalOnly = OmitThisParameter<
  (this: { tag: "optional" }, value?: number) => void
>;
declare const optionalOnly: OptionalOnly;
optionalOnly();
optionalOnly(1);
optionalOnly("wrong"); // error[TK2345]

type OptionalAndRest = OmitThisParameter<
  (this: { tag: "rest" }, value?: number, ...tail: boolean[]) => void
>;
declare const optionalAndRest: OptionalAndRest;
optionalAndRest();
optionalAndRest(1, true, false);
optionalAndRest("wrong"); // error[TK2345]

type OmitAlias<T> = OmitThisParameter<T>;
type NestedOmitAlias<T> = OmitAlias<T>;
type AliasedOptional = NestedOmitAlias<
  (this: { tag: "alias" }, value?: number) => void
>;
declare const aliasedOptional: AliasedOptional;
aliasedOptional();

type ErasedUnconstrained = OmitThisParameter<
  <T>(this: T, value: T) => T
>;
declare const erasedUnconstrained: ErasedUnconstrained;
erasedUnconstrained(1); // error[TK2684]: The 'this' context
erasedUnconstrained(undefined);
erasedUnconstrained(); // error[TK2554]
const unconstrainedHolder = { erasedUnconstrained };
unconstrainedHolder.erasedUnconstrained(unconstrainedHolder);
unconstrainedHolder.erasedUnconstrained(1); // error[TK2684]

type ErasedConstrained = OmitThisParameter<
  <T extends { n: number }>(this: T, value: T) => T
>;
declare const erasedConstrained: ErasedConstrained;
const constrainedResult: { n: number } = erasedConstrained({ n: 1 });
erasedConstrained({ n: "wrong" }); // error[TK2322]

type ErasedDefault = OmitThisParameter<
  <T = { n: number }>(this: T, value: T) => T
>;
declare const erasedDefault: ErasedDefault;
erasedDefault({ n: 1 }); // error[TK2684]

type ErasedUnknown = OmitThisParameter<
  <T extends unknown>(this: T, value: T) => T
>;
declare const erasedUnknown: ErasedUnknown;
erasedUnknown(1); // error[TK2684]

type ErasedAny = OmitThisParameter<
  <T extends any>(this: T, value: T) => T
>;
declare const erasedAny: ErasedAny;
erasedAny(1); // error[TK2684]

type GuardedUnionInput =
  | (<T>(this: T, value: T) => T)
  | { readonly notCallable: true };
type GuardedUnionOmit = OmitThisParameter<GuardedUnionInput>;
declare const guardedUnionOmit: GuardedUnionOmit;
const guardedUnionControl: GuardedUnionInput = guardedUnionOmit;
// The generic receiver makes the intrinsic guard retain this union. tsc reports TS2349 here;
// typokat's union-call diagnostic is deferred to backlog 19.
guardedUnionOmit(1);

type GenericWithoutReceiver = <T>(value: T) => T;
type PreservedGeneric = OmitThisParameter<GenericWithoutReceiver>;
declare const preservedGeneric: PreservedGeneric;
const preservedResult: number = preservedGeneric(1);

type ReceiverOverload = {
  (this: { tag: "first" }, value: string): number;
  (this: { tag: "last" }, value: number): string;
};

type LastReceiver = ThisParameterType<ReceiverOverload>;
const lastReceiverOk: LastReceiver = { tag: "last" };
const lastReceiverBad: LastReceiver = { tag: "first" }; // error[TK2322]

type OmittedOverload = OmitThisParameter<ReceiverOverload>;
declare const omittedOverload: OmittedOverload;
const overloadOk: string = omittedOverload(1);
omittedOverload("wrong"); // error[TK2345]

type OptionalRestOverload = {
  (this: { tag: "first" }, value: string, optional?: boolean): number;
  (this: { tag: "last" }, value: number, optional?: string, ...tail: boolean[]): string;
};
type OmittedOptionalRestOverload = OmitThisParameter<OptionalRestOverload>;
declare const omittedOptionalRestOverload: OmittedOptionalRestOverload;
const optionalRestOverloadOk: string = omittedOptionalRestOverload(1);
omittedOptionalRestOverload(1, "ok", true, false);
omittedOptionalRestOverload(); // error[TK2555]
omittedOptionalRestOverload("wrong"); // error[TK2345]
