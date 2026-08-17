// tsc 6.0.3 --strict: clean. `satisfies` contextually shapes the source but does not
// replace its type with the target.

const tagged = { kind: "left" } satisfies { kind: "left" | "right" };
const exactTag: "left" = tagged.kind;

const pair = ["name", 1] satisfies [string | number, string | number];
const exactFirst: string = pair[0];
const exactSecond: number = pair[1];

const callback = ((value) => value + 1) satisfies (value: number) => number;
const callbackResult: number = callback(1);

const holder = {
  callback: value => value,
} satisfies {
  callback: (value: number) => number;
};
const holderResult: number = holder.callback(1);

const explicitLiteralCallback =
  ((value: number) => "fixed") satisfies (value: number) => "fixed";
const retainedExplicitReturn: "fixed" = explicitLiteralCallback(1);

const contextualLiteralCallback =
  ((value) => "fixed") satisfies (value: number) => "fixed";
const retainedContextualReturn: "fixed" = contextualLiteralCallback(1);

const structuredReturnCallback =
  ((value: number) => ({ kind: "a", count: value })) satisfies
  (value: number) => { kind: "a" | "b"; count: number };
const retainedStructuredReturn: "a" = structuredReturnCallback(1).kind;

const contextualStructuredReturn =
  ((value) => ({ kind: "a", count: value })) satisfies
  (value: number) => { kind: "a" | "b"; count: number };
const retainedContextualStructuredReturn: "a" = contextualStructuredReturn(1).kind;

const tupleReturnCallback =
  (() => ["a", 1]) satisfies () => ["a" | "b", number];
const retainedTupleReturn: "a" = tupleReturnCallback()[0];

const blockStructuredReturn = ((value: number) => {
  return { kind: "a", count: value };
}) satisfies (value: number) => { kind: "a" | "b"; count: number };
const retainedBlockStructuredReturn: "a" = blockStructuredReturn(1).kind;

const contextualBlockStructuredReturn = ((value) => {
  return { kind: "a", count: value };
}) satisfies (value: number) => { kind: "a" | "b"; count: number };
const retainedContextualBlockReturn: "a" = contextualBlockStructuredReturn(1).kind;

const blockTupleReturn = (() => {
  return ["a", 1];
}) satisfies () => ["a" | "b", number];
const retainedBlockTupleReturn: "a" = blockTupleReturn()[0];

const annotatedBlockReturn = ((value: number): { kind: "a"; count: number } => {
  return { kind: "a", count: value };
}) satisfies (value: number) => { kind: "a" | "b"; count: number };
const retainedAnnotatedBlockReturn: "a" = annotatedBlockReturn(1).kind;

const annotatedUnionReturn =
  ((value: number): { kind: "a" | "b" } => ({ kind: "a" })) satisfies
  (value: number) => { kind: "a" | "b" };
const narrowedAnnotatedReturn: "a" = annotatedUnionReturn(1).kind; // error[TK2322]

const contextualAnnotatedUnionReturn =
  ((value): { kind: "a" | "b" } => ({ kind: value > 0 ? "a" : "b" })) satisfies
  (value: number) => { kind: "a" | "b" };
const narrowedContextualAnnotatedReturn: "a" = contextualAnnotatedUnionReturn(1).kind; // error[TK2322]

const widenedObject = { text: "value", count: 1 } satisfies { text: string; count: number };
const exactText: "value" = widenedObject.text; // error[TK2322]: Type 'string' is not assignable to type '"value"'
const exactCount: 1 = widenedObject.count; // error[TK2322]: Type 'number' is not assignable to type '1'

const widenedTuple = ["value", 1] satisfies [string | number, string | number];
const exactTupleText: "value" = widenedTuple[0]; // error[TK2322]: Type 'string' is not assignable to type '"value"'
const exactTupleCount: 1 = widenedTuple[1]; // error[TK2322]: Type 'number' is not assignable to type '1'

const widenedArray = ["left", "right"] satisfies string[];
const exactArrayElement: "left" | "right" = widenedArray[0]; // error[TK2322]

const rawBoolean = { flag: true } satisfies { flag: boolean };
const retainedRawBoolean: true = rawBoolean.flag;

const typedText: "typed" = "typed";
const typedCount: 2 = 2;
const typedObject = { text: typedText, count: typedCount } satisfies {
  text: string;
  count: number;
};
const retainedTypedText: "typed" = typedObject.text;
const retainedTypedCount: 2 = typedObject.count;

const assertedObject = { text: "asserted" as "asserted", count: 3 as 3 } satisfies {
  text: string;
  count: number;
};
const retainedAssertedText: "asserted" = assertedObject.text;
const retainedAssertedCount: 3 = assertedObject.count;

const typedTuple = [typedText, typedCount] satisfies [string, number];
const retainedTupleText: "typed" = typedTuple[0];
const retainedTupleCount: 2 = typedTuple[1];

const typedArray = [typedText] satisfies string[];
const retainedArrayText: "typed" = typedArray[0];

class Published {
  field = { kind: "left" } satisfies { kind: "left" | "right" };
}

declare const published: Published;
const publishedTag: "left" = published.field.kind;

class PublishedExplicitCallback {
  field = {
    callback: (value: number) => value,
  } satisfies {
    callback: (value: number) => number;
  };
}

declare const publishedExplicit: PublishedExplicitCallback;
const explicitResult: number = publishedExplicit.field.callback(1);

class PublishedContextualCallback {
  field = {
    callback: value => value,
  } satisfies {
    callback: (value: number) => number;
  };
}

declare const publishedContextual: PublishedContextualCallback;
const contextualResult: number = publishedContextual.field.callback(1);

class PublishedLiteralProvenance {
  field = { flag: true, text: "fixed" as "fixed" } satisfies {
    flag: boolean;
    text: string;
  };
}

declare const publishedLiteralProvenance: PublishedLiteralProvenance;
const publishedFlag: true = publishedLiteralProvenance.field.flag;
const publishedText: "fixed" = publishedLiteralProvenance.field.text;

class PublishedNestedIdentifiers {
  field = {
    text: typedText,
    tuple: [typedCount],
    array: [typedText],
  } satisfies {
    text: string;
    tuple: [number];
    array: string[];
  };
}

declare const publishedNestedIdentifiers: PublishedNestedIdentifiers;
const publishedNestedText: "typed" = publishedNestedIdentifiers.field.text;
const publishedNestedTuple: 2 = publishedNestedIdentifiers.field.tuple[0];
const publishedNestedArray: "typed" = publishedNestedIdentifiers.field.array[0];

class PublishedLiteralCallback {
  field = ((value) => "fixed") satisfies (value: number) => "fixed";
}

declare const publishedLiteralCallback: PublishedLiteralCallback;
const publishedLiteralReturn: "fixed" = publishedLiteralCallback.field(1);

declare function needNumber(value: number): number;
needNumber("bad") satisfies number; // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
