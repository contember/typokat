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
