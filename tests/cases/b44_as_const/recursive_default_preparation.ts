declare function prepareActiveDefault<T extends { u: U }, U = T>(value?: T): [T, U];
const activeDefaultResult = prepareActiveDefault();
const activeDefaultShape: { u: unknown } = activeDefaultResult[0];
const activeDefaultWrong: number = activeDefaultResult[1]; // error[TK2322]

const suppliedDefaultResult = prepareActiveDefault(1 as const); // error[TK2345]
const suppliedDefaultShape: { u: 1 } = suppliedDefaultResult[0];
const suppliedDefaultSeed: 1 = suppliedDefaultResult[1];
const suppliedDefaultWrong: 2 = suppliedDefaultResult[1]; // error[TK2322]

declare function prepareDemandingDefault<A extends U, T extends { u: U }, U = T>(): [A, T, U];
const demandingDefaultResult = prepareDemandingDefault();
const demandingDefaultShapes: [{ u: unknown }, { u: unknown }, { u: unknown }] = demandingDefaultResult;
const demandingDefaultWrong: number = demandingDefaultResult[2].u; // error[TK2322]

declare function prepareCompletedDefault<T extends string, U = T>(value: T): [T, U];
const completedDefaultResult = prepareCompletedDefault(1 as const); // error[TK2345]
const completedDefaultValue: string = completedDefaultResult[1];
const completedDefaultWrong: number = completedDefaultResult[1]; // error[TK2322]
