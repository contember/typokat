export {};

const tag: string = /global/.b103Tag();
const wrongTag: number = /global/.b103Tag(); // error[TK2322]: Type 'string' is not assignable to type 'number'
const tested: boolean = /global/.test("global");
const wrongTested: string = /global/.test("global"); // error[TK2322]: Type 'boolean' is not assignable to type 'string'
