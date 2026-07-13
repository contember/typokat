// Semantic-duplication sprint WU0 — call/construct candidate precedence.
// Cross-checked with tsc 6.0.3 --strict. Deliberate wording/code differences are
// recorded in docs/reference/divergences.md.

interface ConstraintBeforeArityCall {
  <T extends string>(value: T): "constraint";
  <T extends boolean>(value: T, extra: number): "arity";
}

declare const constraintBeforeArityCall: ConstraintBeforeArityCall;
constraintBeforeArityCall<boolean>(true); // error[TK2344]: Type 'boolean' does not satisfy the constraint 'string'

interface ConstraintBeforeArityConstruct {
  new <T extends string>(value: T): { kind: "constraint" };
  new <T extends boolean>(value: T, extra: number): { kind: "arity" };
}

declare const ConstraintBeforeArityConstructor: ConstraintBeforeArityConstruct;
new ConstraintBeforeArityConstructor<boolean>(true); // error[TK2344]: Type 'boolean' does not satisfy the constraint 'string'

interface MixedFailureCall {
  (value: number): void;
  (value: string, extra: string): void;
}

declare const mixedFailureCall: MixedFailureCall;
mixedFailureCall(true); // error[TK2769]: No overload matches this call

interface MixedFailureConstruct {
  new (value: number): { kind: "number" };
  new (value: string, extra: string): { kind: "string" };
}

declare const MixedFailureConstructor: MixedFailureConstruct;
new MixedFailureConstructor(true); // error[TK2769]: No overload matches this call

interface PureArityCall {
  (value: number): void;
  (value: string, extra: string): void;
}

declare const pureArityCall: PureArityCall;
pureArityCall(); // error[TK2554]: Expected 1-2 arguments, but got 0

interface PureArityConstruct {
  new (value: number): { kind: "number" };
  new (value: string, extra: string): { kind: "string" };
}

declare const PureArityConstructor: PureArityConstruct;
new PureArityConstructor(); // error[TK2554]: Expected 1-2 arguments, but got 0

interface ReceiverSelector {
  tag: "ok";
  select(this: { tag: "bad" }, value: number): "bad";
  select(this: { tag: "ok" }, value: number): "ok";
}

declare const receiverSelector: ReceiverSelector;
const selectedReceiver: "ok" = receiverSelector.select(1);
const detachedReceiver = receiverSelector.select;
detachedReceiver(1); // error[TK2769]: No overload matches this call

interface ExplicitConstraintOrderCall {
  <T extends "first">(value: T): "first";
  <T extends number>(value: T): "last";
}

declare const explicitConstraintOrderCall: ExplicitConstraintOrderCall;
explicitConstraintOrderCall<boolean>(true); // error[TK2344]: Type 'boolean' does not satisfy the constraint '"first"'

interface ExplicitConstraintOrderConstruct {
  new <T extends "first">(value: T): { kind: "first" };
  new <T extends number>(value: T): { kind: "last" };
}

declare const ExplicitConstraintOrderConstructor: ExplicitConstraintOrderConstruct;
new ExplicitConstraintOrderConstructor<boolean>(true); // error[TK2344]: Type 'boolean' does not satisfy the constraint '"first"'
