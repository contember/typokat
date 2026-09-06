class ReadyRestArrowFields {
  array = ((value) => value) satisfies (...args: number[]) => number;
  tuple = ((value) => value) satisfies (...args: [number]) => number;
  mixed = ((first, second) => second) satisfies (...args: [number, string]) => string;
  prefix = ((first, second) => second) satisfies (first: string, ...args: number[]) => number;
  repeated = ((first, second, third) => third) satisfies (...args: number[]) => number;
  constant = ((value) => 1) satisfies (...args: number[]) => number;
  declaredReturn = ((value): number => 1) satisfies (...args: number[]) => number;
  declaredParameter = ((value: number) => value) satisfies (...args: number[]) => number;
  tooWideReturn = ((value) => value) satisfies (...args: number[]) => number[]; // error[TK2322]
  wrongTupleReturn = ((first, second) => second) satisfies (...args: [number, string]) => number; // error[TK2322]
  wrongDeclaredReturn = ((value): string => value) satisfies (...args: number[]) => string; // error[TK2322]
}

const readyRestArrows = new ReadyRestArrowFields();
const arrayResult: number = readyRestArrows.array(1);
const tupleResult: number = readyRestArrows.tuple(1);
const mixedResult: string = readyRestArrows.mixed(1, "a");
const prefixResult: number = readyRestArrows.prefix("a", 1);
const repeatedResult: number = readyRestArrows.repeated(1, 2, 3);
readyRestArrows.array("bad"); // error[TK2345]
readyRestArrows.tuple("bad"); // error[TK2345]
readyRestArrows.mixed(1, 2); // error[TK2345]
readyRestArrows.prefix("a", "bad"); // error[TK2345]
readyRestArrows.repeated(1, 2, "bad"); // error[TK2345]
