// tsc 6.0.3 --strict --target es2025: TS2322 x2 and TS2339 below.

const expression: RegExp = /type(?:script)?/gi;
const matches: boolean = expression.test("TypeScript");
const wrongMatches: string = expression.test("TypeScript"); // error[TK2322]: Type 'boolean' is not assignable to type 'string'

const execution: RegExpExecArray | null = expression.exec("TypeScript");
const wrongExecution: number = expression.exec("TypeScript"); // error[TK2322]
expression.notARegExpMember; // error[TK2339]
