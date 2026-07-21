// Backlog 32 — `keyof` must not bake `never` while its operand is only a
// forward declaration. Cross-checked with tsc 6.0.3 --strict --noEmit.

interface ElementBase {}
interface DivElement extends ElementBase {
  align: string;
}
interface SpanElement extends ElementBase {
  spanOnly: number;
}

// Production shape: lib.dom declares Document before HTMLElementTagNameMap.
interface ForwardDocument {
  createElement<K extends keyof ForwardTagMap>(tagName: K): ForwardTagMap[K];
  createElement(tagName: string): ElementBase;
}
interface ForwardTagMap {
  div: DivElement;
  span: SpanElement;
}

declare const forwardDocument: ForwardDocument;
const forwardDiv: DivElement = forwardDocument.createElement("div");
const forwardSpan: SpanElement = forwardDocument.createElement("span");
const forwardWrongElement: DivElement = forwardDocument.createElement("span"); // error[TK2741]: Property 'align' is missing
const forwardMissing: DivElement = forwardDocument.createElement("missing"); // error[TK2741]: Property 'align' is missing

// Eager-constraint control: literal-union inference selects the same map branches.
interface ReverseTagMap {
  div: DivElement;
  span: SpanElement;
}
interface ReverseDocument {
  createElement<K extends "div" | "span">(tagName: K): ReverseTagMap[K];
  createElement(tagName: string): ElementBase;
}

declare const reverseDocument: ReverseDocument;
const reverseDiv: DivElement = reverseDocument.createElement("div");
const reverseSpan: SpanElement = reverseDocument.createElement("span");
const reverseWrongElement: DivElement = reverseDocument.createElement("span"); // error[TK2741]: Property 'align' is missing
const reverseMissing: DivElement = reverseDocument.createElement("missing"); // error[TK2741]: Property 'align' is missing

// Direct soundness guard: a forward `keyof` annotation must reject non-keys.
interface ForwardKeyHolder {
  key: keyof LaterKeys;
}
interface LaterKeys {
  alpha: number;
  beta: string;
}

const validForwardKey: ForwardKeyHolder = { key: "alpha" };
const invalidForwardKey: ForwardKeyHolder = { key: "missing" }; // error[TK2322]

// Reverse-order control for the direct annotation route.
interface EarlierKeys {
  alpha: number;
  beta: string;
}
interface ReverseKeyHolder {
  key: keyof EarlierKeys;
}

const validReverseKey: ReverseKeyHolder = { key: "beta" };
const invalidReverseKey: ReverseKeyHolder = { key: "missing" }; // error[TK2322]

// Forward class operand.
interface ForwardClassKeyHolder {
  key: keyof LaterClassKeys;
}
declare class LaterClassKeys {
  alpha: number;
  beta: string;
}

const validForwardClassKey: ForwardClassKeyHolder = { key: "alpha" };
const invalidForwardClassKey: ForwardClassKeyHolder = { key: "missing" }; // error[TK2322]

// Forward alias operand.
interface ForwardAliasKeyHolder {
  key: keyof LaterAliasKeys;
}
type LaterAliasKeys = {
  alpha: number;
  beta: string;
};

const validForwardAliasKey: ForwardAliasKeyHolder = { key: "beta" };
const invalidForwardAliasKey: ForwardAliasKeyHolder = { key: "missing" }; // error[TK2322]

// Heritage forces the forward class base to fill before its key operand exists.
interface HeritageView extends HeritageBase {}
declare class HeritageBase {
  key: keyof LaterHeritageKeys;
}
interface LaterHeritageKeys {
  alpha: number;
  beta: string;
}

const validHeritageKey: HeritageView = { key: "alpha" };
const invalidHeritageKey: HeritageView = { key: "missing" }; // error[TK2322]

// Mutual shapes must observe each other's eventual member sets.
interface MutualLeft {
  left: number;
  keyOfRight: keyof MutualRight;
}
interface MutualRight {
  right: string;
  keyOfLeft: keyof MutualLeft;
}

const validMutualLeft: MutualLeft = { left: 1, keyOfRight: "right" };
const invalidMutualLeft: MutualLeft = { left: 1, keyOfRight: "missing" }; // error[TK2322]
const validMutualRight: MutualRight = { right: "x", keyOfLeft: "left" };
const invalidMutualRight: MutualRight = { right: "x", keyOfLeft: "missing" }; // error[TK2322]

// Standalone indexed access audit: the same forward operand must preserve the
// selected value type.
interface ForwardIndexedHolder {
  value: LaterIndexedValues["alpha"];
}
interface LaterIndexedValues {
  alpha: number;
  beta: string;
}

const validForwardIndexed: ForwardIndexedHolder = { value: 1 };
const invalidForwardIndexed: ForwardIndexedHolder = { value: "x" }; // error[TK2322]
