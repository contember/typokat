// A contextual generic source retains its binders; row relations do not admit Cartesian calls.

type GenericContextRows = <T extends ["a", number] | ["b", string]>(...args: T) => void;
const preservedGenericRows = ((kind, payload) => {
  if (kind === "a") payload.toFixed();
}) satisfies GenericContextRows;
preservedGenericRows<["a", number]>("a", 1);
preservedGenericRows<["a", number]>("b", 1); // error[TK2345]
preservedGenericRows<["a", 1]>("a", 1);
preservedGenericRows<["a", 1]>("a", 2); // error[TK2345]

const annotatedGenericRows: GenericContextRows = (kind, payload) => {
  if (kind === "a") payload.toFixed();
};
annotatedGenericRows("a", "wrong"); // error[TK2345]

const broadRowImplementation: (kind: "a" | "b", payload: number | string) => void = (kind, payload) => {};
const validGenericRowTarget: GenericContextRows = broadRowImplementation;
const narrowRowImplementation: (kind: "a", payload: number) => void = (kind, payload) => {};
const invalidGenericRowTarget: GenericContextRows = narrowRowImplementation; // error[TK2322]

declare const concreteRowSource: (...args: ["a", number] | ["b", string]) => void;
const invalidCartesianTarget: (kind: "a" | "b", payload: number | string) => void = concreteRowSource; // error[TK2322]

const concreteIdentity: (value: string | number) => string | number = (value) => value;
const invalidUniversalIdentity: <T>(value: T) => T = concreteIdentity; // error[TK2322]
