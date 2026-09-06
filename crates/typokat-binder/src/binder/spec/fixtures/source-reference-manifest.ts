declare function producer<T>(): T;
declare const values: unknown[];
declare const key: string;

const alias = /* use: value,copy */ producer;
const parenthesized = (((/* use: value,copy */ producer)));
const sink: unknown = /* use: value,unknown-sink */ producer;
const callable: () => unknown = /* use: value,callable-sink */ producer;
const result = /* use: value,call */ producer();
const explicit = /* use: value,call-type-arguments */ producer<never>();
const optional = /* use: value,optional-call */ producer?.();
const nested = consume(/* use: value,argument */ producer);
const asserted = /* use: value,assertion */ producer as unknown;
const checked = /* use: value,satisfies */ producer satisfies () => unknown;
const shorthand = { /* use: value,shorthand */ producer };
const computed = { [/* use: value,computed-key */ producer]: 1 };
const spread = { .../* use: value,spread */ producer };
const keysAreNotReads = { producer: 1, "producer": 2 };
const textIsNotARead = "producer";

type Query = typeof /* use: query,value-query */ producer;
type QualifiedQuery = typeof /* use: query,value-query */ producer.member;
type producer = number;
type TypeUse = /* use: type,type-reference */ producer;
type Imported = import("external").producer;

/* use: write,assignment */ producer = alias;
/* use: read-write,update */ producer++;
({ value: /* use: write,destructuring */ producer } = source);
for (/* use: write,loop-target */ producer of values) {}
if (false) { consume(/* use: value,argument */ producer); }
producer: while (false) { break producer; }

function captured() { return /* use: value,return */ producer; }
const arrow = (): typeof /* use: query,value-query */ producer => /* use: value,return */ producer;
function defaulted(value = /* use: value,default */ producer) {}
function patterned({ value = /* use: value,default */ producer }) {}
function shadowed() {
  const producer = 1;
  return /* use: value,return */ producer;
}

@decorate(/* use: value,argument */ producer)
class Holder {
  field = /* use: value,initializer */ producer;
  static { consume(/* use: value,argument */ producer); }
  [/* use: value,computed-key */ producer]() {}
}

namespace Nested { export const reference = /* use: value,initializer */ producer; }
declare module "ambient" { type Captured = typeof /* use: query,value-query */ producer; }
export { /* use: export,local-export */ producer as publicName };
export { alias as producer };
const globals = globalThis;
const globalValue = globalThis[key];
