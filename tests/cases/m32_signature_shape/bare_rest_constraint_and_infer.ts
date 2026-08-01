// M32 full-library cutover: bare `any` / `never` rest operands are ordinary
// function-shape semantics, not special cases for the built-in utility names.
// Cross-checked with tsc 6.0.3 --strict --lib es5.

type AcceptCallable<T extends (...args: any) => any> = T;

type AcceptedFixed = AcceptCallable<(value: number) => string>;
declare const acceptedFixed: AcceptedFixed;
const acceptedFixedResult: string = acceptedFixed(1);
const rejectedFixedResult: number = acceptedFixed(1); // error[TK2322]

type CallableObject = {
  (value: number): boolean;
  readonly kind: "callable";
};
type AcceptedObject = AcceptCallable<CallableObject>;
declare const acceptedObject: AcceptedObject;
const acceptedObjectResult: boolean = acceptedObject(1);
const rejectedObjectResult: string = acceptedObject(1); // error[TK2322]

type RejectedNonCallable = AcceptCallable<{ readonly kind: "not-callable" }>; // error[TK2344]

type InferReturn<T> = T extends (...args: any) => infer Result ? Result : never;
type InferredFixedReturn = InferReturn<(value: number) => string>;
const inferredFixedOk: InferredFixedReturn = "ok";
const inferredFixedWrong: InferredFixedReturn = 1; // error[TK2322]

type OverloadedCallable = {
  (value: string): number;
  (value: number): string;
};
type InferredOverloadReturn = InferReturn<OverloadedCallable>;
const inferredOverloadOk: InferredOverloadReturn = "last";
const inferredOverloadWrong: InferredOverloadReturn = 1; // error[TK2322]

type InferReceiver<T> = T extends (this: infer Receiver, ...args: never) => any
  ? Receiver
  : unknown;
type InferredReceiver = InferReceiver<
  (this: { readonly tag: "receiver" }, value: number) => string
>;
const inferredReceiverOk: InferredReceiver = { tag: "receiver" };
const inferredReceiverWrong: InferredReceiver = { tag: "wrong" }; // error[TK2322]

type MissingReceiver = InferReceiver<(value: number) => string>;
const missingReceiverIsUnknown: MissingReceiver = 1;

// A typed array rest remains selective; repairing bare `any` must not make every
// variadic target a wildcard.
type AcceptStringRest<T extends (...args: string[]) => unknown> = T;
type AcceptedStringRest = AcceptStringRest<(value: string) => number>;
declare const acceptedStringRest: AcceptedStringRest;
const acceptedStringRestResult: number = acceptedStringRest("ok");
type RejectedStringRest = AcceptStringRest<(value: number) => number>; // error[TK2344]
