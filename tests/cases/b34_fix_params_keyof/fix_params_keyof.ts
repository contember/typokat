declare function pick<T, K extends keyof T>(obj: T, key: K): void;

pick({ a: 1, b: 2 }, "c"); // error[TK2345]
pick({ a: 1, b: 2 }, "a");

interface Pair {
  left: number;
  right: string;
}

declare const pair: Pair;
pick(pair, "middle"); // error[TK2345]
pick(pair, "left");

function wrapper<T>(obj: T, key: keyof T) {
  pick(obj, key);
}

declare const unionObj: { a: number } | { b: string };
pick(unionObj, "a"); // error[TK2345]
pick(unionObj, "b"); // error[TK2345]

declare const commonObj: { a: number; shared: number } | { b: string; shared: number };
pick(commonObj, "shared");
pick(commonObj, "a"); // error[TK2345]

type Inf<T> = T extends {} ? Inf<{ v: T }> : never;
declare function deep<T, K extends Inf<T>>(obj: T, key: K): void;
deep({}, 0); // error[TK2589]

declare function pickObj<T, K extends { key: keyof T }>(obj: T, k: K): void;
declare const good: { key: "a" };
declare const bad: { key: "b" };
pickObj({ a: 1 }, good);
pickObj({ a: 1 }, bad); // error[TK2345]

type A = { kind: "a"; shared: number; onlyA: string };
type B = { kind: "b"; shared: number; onlyB: boolean };
type P = Pick<A | B, "shared">;
const p1: P = { shared: 1 };
const p2: P = { shared: "x" }; // error[TK2322]

type IndexedA = { [k: string]: number };
type IndexedB = { a: string };
type IndexedPick = Pick<IndexedA | IndexedB, "a">;
const indexed1: IndexedPick = { a: 1 };
const indexed2: IndexedPick = { a: "x" };
const indexed3: IndexedPick = { a: true }; // error[TK2322]
