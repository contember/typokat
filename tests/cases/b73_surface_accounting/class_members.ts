// Surface-accounting spec (backlog 73 / 75). ENABLED by WU5: class member collection
// records the incomplete surface for members it does not model before dropping them. See
// tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `collect_class_own_members` silently `_ => {}`-dropped static blocks,
// auto-accessors, and class index signatures, and `continue`-d past computed method and
// property-definition keys (the property arm was the WU7-E F2 finding). Each is now
// recorded — a static-block body is otherwise a silent false-clean.

const key = "run";
const pkey = "prop";

class A {
  // INCOMPLETE: the static block body is not walked (a bad statement here is false-clean).
  static { const bad: number = 1; } // incomplete[class/static-block/self]

  // INCOMPLETE: an auto-accessor field is not checked.
  accessor auto: number = 0; // incomplete[class/accessor-property/self]

  // INCOMPLETE: a class index signature is not collected.
  [idx: string]: number; // incomplete[class/class-index-signature/self]

  // INCOMPLETE: a computed method key is not collected into the class type.
  [key]() {} // incomplete[class/method-definition/computed-key]

  // INCOMPLETE (F2): a computed property-definition key is not collected either.
  [pkey]: number; // incomplete[class/property-definition/computed-key]

  // CONTROL (supported): a plain method is collected — no incomplete, clean.
  plain(): number {
    return 1;
  }
}
