// tsc additionally reports TS2339 x2 after TS2397; the shipped corpus preserves both failures.
const explicitEnabled: boolean = globalThis.B14ExplicitGlobalThis.enabled; // error[TK2339]
const wrongExplicitEnabled: string = globalThis.B14ExplicitGlobalThis.enabled; // error[TK2339]
