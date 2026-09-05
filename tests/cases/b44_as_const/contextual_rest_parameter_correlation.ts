// Rest-tuple alternatives preserve the relation between each contextual parameter.

const correlatedPrimitiveCallback: (...args: ["A", number] | ["B", string]) => void = (kind, payload) => {
  if (kind === "A") {
    payload.toFixed();
    payload.toUpperCase(); // error[TK2339]
  }
  if (kind === "B") {
    payload.toUpperCase();
    const wrongPrimitivePayload: number = payload; // error[TK2322]
  }
};

const correlatedObjectCallback: (...args: ["A", { a: number }] | ["B", { b: string }]) => void = (kind, payload) => {
  if (kind === "A") {
    const numberPayload: number = payload.a;
    payload.b; // error[TK2339]
  }
  if (kind === "B") {
    const stringPayload: string = payload.b;
    payload.a; // error[TK2339]
  }
};

type CorrelatedGenericCallback = <T extends ["a", number] | ["b", string]>(...args: T) => void;
const correlatedGenericCallback: CorrelatedGenericCallback = (kind, payload) => {
  if (kind === "a") {
    payload.toFixed();
    payload.toUpperCase(); // error[TK2339]
  }
  if (kind === "b") {
    payload.toUpperCase();
    const wrongGenericPayload: number = payload; // error[TK2322]
  }
};
