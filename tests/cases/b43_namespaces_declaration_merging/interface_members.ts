// tsc 6.0.3 --strict: TS2769 x3 and TS2322 x6 below; all other lines clean.
interface MergedShape<T extends { id: number } = { id: number }> {
  first: number;
  method(value: number): "number";
  (value: number): "number";
  new (value: number): { kind: "number" };
  [key: string]: unknown;
}

interface MergedShape<T extends { id: number } = { id: number }> {
  second: string;
  payload: T;
  method(value: string): "string";
  (value: string): "string";
  new (value: string): { kind: "string" };
  [index: number]: T;
}

declare const merged: MergedShape<{ id: number; extra: string }>;
const wrongFirst: string = merged.first; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongSecond: number = merged.second; // error[TK2322]: Type 'string' is not assignable to type 'number'
const payloadExtra: string = merged.payload.extra;
const methodFromFirstBlock: "number" = merged.method(1);
const methodFromSecondBlock: "string" = merged.method("one");
const callFromFirstBlock: "number" = merged(1);
const callFromSecondBlock: "string" = merged("one");
const constructFromFirstBlock: { kind: "number" } = new merged(1);
const constructFromSecondBlock: { kind: "string" } = new merged("one");
merged.method(true); // error[TK2769]
merged(true); // error[TK2769]
new merged(true); // error[TK2769]
const indexedPayload: string = merged[0].extra;
const wrongIndexPayload: number = merged[0].extra; // error[TK2322]: Type 'string' is not assignable to type 'number'

interface MethodOrder {
  choose(value: "same"): "earlier";
}
interface MethodOrder {
  choose(value: "same"): "later";
}
declare const methodOrder: MethodOrder;
const methodOrderCorrect: "earlier" = methodOrder.choose("same");
const methodOrderWrong: "later" = methodOrder.choose("same"); // error[TK2322]: Type '"earlier"' is not assignable to type '"later"'

interface CallOrder {
  (value: "same"): "earlier";
}
interface CallOrder {
  (value: "same"): "later";
}
declare const callOrder: CallOrder;
const callOrderWrong: "later" = callOrder("same"); // error[TK2322]: Type '"earlier"' is not assignable to type '"later"'
const callOrderCorrect: "earlier" = callOrder("same");

interface ConstructOrder {
  new (value: "same"): { kind: "earlier" };
}
interface ConstructOrder {
  new (value: "same"): { kind: "later" };
}
declare const constructOrder: ConstructOrder;
const constructOrderCorrect: { kind: "earlier" } = new constructOrder("same");
const constructOrderWrong: { kind: "later" } = new constructOrder("same"); // error[TK2322]

interface ReversedGroups {
  method(value: string): "string-first";
  (value: string): "string-first";
  new (value: string): { kind: "string-first" };
}
interface ReversedGroups {
  method(value: number): "number-second";
  (value: number): "number-second";
  new (value: number): { kind: "number-second" };
}
declare const reversedGroups: ReversedGroups;
const reversedMethodFirst: "string-first" = reversedGroups.method("one");
const reversedMethodSecond: "number-second" = reversedGroups.method(1);
const reversedCallFirst: "string-first" = reversedGroups("one");
const reversedCallSecond: "number-second" = reversedGroups(1);
const reversedConstructFirst: { kind: "string-first" } = new reversedGroups("one");
const reversedConstructSecond: { kind: "number-second" } = new reversedGroups(1);
