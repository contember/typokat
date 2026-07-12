// Backlog 41 WU4 regression — tsc accepts these overload implementations.
// Cross-checked with tsc 6.0.3 --strict.

function fixedGenericImplementation(value: string): string;
function fixedGenericImplementation<T>(value: T): T { return value; }

function constrainedGenericReturn<T extends string>(value: T): string;
function constrainedGenericReturn<T extends string>(value: T) { return value; }

const fixedControl: string = fixedGenericImplementation("value");
const constrainedControl: string = constrainedGenericReturn("value");

function invalidFixedGenericImplementation(value: string): string; // error[TK2394]: not compatible with its implementation signature
function invalidFixedGenericImplementation<T>(value: T): number { return 1; }

function invalidConstrainedGenericArity<T extends string>(value: T): T; // error[TK2394]: not compatible with its implementation signature
function invalidConstrainedGenericArity<T extends string>(value: T, extra: number): T { return value; }
