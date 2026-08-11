// Backlog 109 — every shared call consumer admits undefined at an optional fixed slot.
// Cross-checked with tsc 6.0.3 --strict --noEmit.

class OptionalConsumers {
  constructor(value?: number) {}

  method(value?: number): void {}
}

const instance = new OptionalConsumers(undefined);

const functionValue: (value?: number) => void = (value?: number): void => {};
functionValue(undefined);

class DefaultConsumers {
  constructor(value: number = 1) {}

  method(value: number = 1): void {}
}

const defaultInstance = new DefaultConsumers(undefined);
defaultInstance.method(undefined);

class OptionalBase {
  constructor(value?: number) {}
}

class OptionalDerived extends OptionalBase {
  constructor() {
    super(undefined);
  }
}

interface OptionalCallSignature {
  (value?: number): "called";
}

declare const callSignature: OptionalCallSignature;
const callResult: "called" = callSignature(undefined);

interface OptionalConstructSignature {
  new (value?: number): { kind: "constructed" };
}

declare const constructSignature: OptionalConstructSignature;

function consumeForwarded(forwarded: number | undefined): void {
  instance.method(forwarded);
  const constructResult: { kind: "constructed" } = new constructSignature(forwarded);
}
