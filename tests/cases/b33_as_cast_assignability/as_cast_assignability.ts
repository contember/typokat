const primitiveBad: number = "hello" as string; // error[TK2322]: Type 'string' is not assignable to type 'number'
const objectBad: { a: number } = {} as {}; // error[TK2741]

let assigned: number = 1;
assigned = "again" as string; // error[TK2322]: Type 'string' is not assignable to type 'number'

declare function takesNumber(x: number): void;
takesNumber("s" as string); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

declare function takesObj(x: { a: number }): void;
takesObj({} as {}); // error[TK2345]

const unresolvedCast: number = missingValue as number; // error[TK2304]
const angleBad: number = <string>"hello"; // error[TK2322]: Type 'string' is not assignable to type 'number'

const legalUpcast: string | number = "ok" as string;

declare function takesOne(x: 1): void;
takesOne(1 as const);
takesOne(2 as const); // error[TK2345]: Argument of type '2' is not assignable to parameter of type '1'
