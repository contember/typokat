// tsc additionally reports TS2339 x2 after TS2397. WU3 must preserve both use-site failures.
const explicitEnabled: boolean = globalThis.B14ExplicitGlobalThis.enabled; // error[TK2339]
const wrongExplicitEnabled: string = globalThis.B14ExplicitGlobalThis.enabled; // error[TK2339]
