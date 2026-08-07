// Sorted after every mutation-capable project. No private rebuild may leak a root, member, global
// object contribution, or unsupported merge into the shared base used by this external module.
// Oracle: TS2304 x11, TS2339 x3, TS7017 x4, and TS2322 x2. This shipped typokat corpus
// normalizes TS7017 to the existing member-missing TK2339; bare absent roots remain TK2304.
export {};

[1, 2].b14Collision; // error[TK2339]
/isolated/.b14Tag; // error[TK2339]
const retainedMap: number[] = [1, 2].map((value) => value + 1);
const wrongRetainedMap: string[] = [1, 2].map((value) => value + 1); // error[TK2322]
const retainedTest: boolean = /isolated/.test("isolated");
const wrongRetainedTest: string = /isolated/.test("isolated"); // error[TK2322]

const isolatedRootNamespace = B14RootNamespace; // error[TK2304]
globalThis.B14RootNamespace; // error[TK2339]

const isolatedDeferredValue = B14DeferredGlobalValue; // error[TK2304]
const isolatedDeferredFunction = B14DeferredGlobalFunction; // error[TK2304]
const isolatedDeferredClass = B14DeferredGlobalClass; // error[TK2304]
const isolatedDuplicateNamespace = B14DuplicateGlobal; // error[TK2304]

globalThis.B14ExplicitGlobalThis; // error[TK2339]
const isolatedUmd = B14Umd; // error[TK2304]
const isolatedUniqueGlobal = B14UniqueGlobal; // error[TK2304]
const isolatedGlobalFunction = B14GlobalFunction; // error[TK2304]
const isolatedGlobalLet = B14GlobalLet; // error[TK2304]
const isolatedGlobalConst = B14GlobalConst; // error[TK2304]
const isolatedGlobalClass = B14GlobalClass; // error[TK2304]
globalThis.B14UniqueGlobal; // error[TK2339]
globalThis.B14GlobalFunction; // error[TK2339]
[1, 2].b14Unsupported; // error[TK2339]
