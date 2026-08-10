// WU2 official-suite regressions: flat leaves project through common-key unions and `any`.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit` reports only the marked assignments,
// missing union property, and unresolved recovery source.

type B48ProjectionUnion =
  | { tag: "count"; value: number; optional?: string }
  | { tag: "text"; value: string; optional?: string };

declare const b48ProjectionUnion: B48ProjectionUnion;
const {
  tag: b48ProjectionTag,
  value: b48ProjectionValue,
  optional: b48ProjectionOptional = "fallback",
} = b48ProjectionUnion;

const b48ProjectionWrongTag: boolean = b48ProjectionTag; // error[TK2322]
const b48ProjectionWrongValue: boolean = b48ProjectionValue; // error[TK2322]
const b48ProjectionWrongOptional: boolean = b48ProjectionOptional; // error[TK2322]

function b48ProjectionGeneric<T extends B48ProjectionUnion>(source: T): void {
  const { tag, value } = source;
  const wrongTag: boolean = tag; // error[TK2322]
  const wrongValue: boolean = value; // error[TK2322]
}

declare const b48ProjectionPartial: { present: number } | { other: string };
const { present: b48ProjectionMissing } = b48ProjectionPartial; // error[TK2339]

declare const b48ProjectionAny: any;
const { leaf: b48ProjectionAnyLeaf, missing: b48ProjectionAnyDefault = 1 } = b48ProjectionAny;
const b48ProjectionAnyNumber: number = b48ProjectionAnyLeaf;
const b48ProjectionAnyString: string = b48ProjectionAnyLeaf;
const b48ProjectionDefaultNumber: number = b48ProjectionAnyDefault;
const b48ProjectionDefaultString: string = b48ProjectionAnyDefault;

declare const b48ProjectionError: B48MissingProjectionSource; // error[TK2304]
const { leaf: b48ProjectionErrorLeaf } = b48ProjectionError; // incomplete[bind/binding-pattern/object-pattern]

// Missing and blocked union members are independent outcomes in either declaration order.
type B48ProjectionMissingFirst = { otherA: string };
type B48ProjectionBlockedSecond = { leaf: B48MissingProjectionA }; // error[TK2304]
declare const b48ProjectionMissingBeforeBlocked:
  | B48ProjectionMissingFirst
  | B48ProjectionBlockedSecond;
const { leaf: b48ProjectionOrderA } = b48ProjectionMissingBeforeBlocked; // error[TK2339] | incomplete[bind/binding-pattern/object-pattern]

type B48ProjectionBlockedFirst = { leaf: B48MissingProjectionB }; // error[TK2304]
type B48ProjectionMissingSecond = { otherB: string };
declare const b48ProjectionBlockedBeforeMissing:
  | B48ProjectionBlockedFirst
  | B48ProjectionMissingSecond;
const { leaf: b48ProjectionOrderB } = b48ProjectionBlockedBeforeMissing; // error[TK2339] | incomplete[bind/binding-pattern/object-pattern]
