// Expanding rest alternatives must preserve aggregate arity and readonly admission.

type ConcreteArityRows = (...args: ["a", number] | ["b", string]) => void;
type GenericArityRows = <T extends ["a", number] | ["b", string]>(...args: T) => void;
declare const noArityParameters: () => void;
declare const oneArityParameter: (kind: "a" | "b") => void;
declare const twoArityParameters: (kind: "a" | "b", value: number | string) => void;
declare const threeArityParameters: (kind: "a" | "b", value: number | string, extra: number) => void;

const concreteNoArity: ConcreteArityRows = noArityParameters;
const concreteOneArity: ConcreteArityRows = oneArityParameter; // error[TK2322]
const concreteTwoArity: ConcreteArityRows = twoArityParameters;
const concreteThreeArity: ConcreteArityRows = threeArityParameters; // error[TK2322]
const genericNoArity: GenericArityRows = noArityParameters;
const genericOneArity: GenericArityRows = oneArityParameter; // error[TK2322]
const genericTwoArity: GenericArityRows = twoArityParameters;
const genericThreeArity: GenericArityRows = threeArityParameters; // error[TK2322]

type ReadonlyArityRows = (...args: readonly ["a", number] | readonly ["b", string]) => void;
const readonlyTwoArity: ReadonlyArityRows = twoArityParameters; // error[TK2322]
