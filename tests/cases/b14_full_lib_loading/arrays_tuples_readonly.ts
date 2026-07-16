// tsc 6.0.3 --strict --target es2025: TS2322 x6 and TS2339 below.

const numbers: number[] = [1, 2, 3];
const doubled: number[] = numbers.map((value) => value * 2);
const wrongDoubled: string[] = numbers.map((value) => value * 2); // error[TK2322]
const wrongElement: string = numbers[0]; // error[TK2322]: Type 'number' is not assignable to type 'string'

const pair: [string, number] = ["age", 42];
const pairLength: 2 = pair.length;
const wrongPairLength: 3 = pair.length; // error[TK2322]
const wrongPairSlot: boolean = pair[1]; // error[TK2322]: Type 'number' is not assignable to type 'boolean'

const readonlyNumbers: readonly number[] = numbers;
const readonlyMapped: number[] = readonlyNumbers.map((value) => value + 1);
const wrongReadonlyMapped: string[] = readonlyNumbers.map((value) => value + 1); // error[TK2322]
readonlyNumbers.push(4); // error[TK2339]

const readonlyPair: readonly [string, number] = pair;
const wrongReadonlySlot: number = readonlyPair[0]; // error[TK2322]: Type 'string' is not assignable to type 'number'
