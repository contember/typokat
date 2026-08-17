// tsc 6.0.3 --strict: clean. `satisfies` contextually shapes the source but does not
// replace its type with the target.

const tagged = { kind: "left" } satisfies { kind: "left" | "right" };
const exactTag: "left" = tagged.kind;

const pair = ["name", 1] satisfies [string | number, string | number];
const exactFirst: string = pair[0];
const exactSecond: number = pair[1];

const callback = ((value) => value + 1) satisfies (value: number) => number;
const callbackResult: number = callback(1);

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

declare function needNumber(value: number): number;
needNumber("bad") satisfies number; // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
