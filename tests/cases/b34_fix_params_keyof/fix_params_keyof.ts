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
