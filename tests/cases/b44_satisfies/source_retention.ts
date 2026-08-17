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

declare function needNumber(value: number): number;
needNumber("bad") satisfies number; // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
