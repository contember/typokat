const constructed: RegExp = new RegExp("x");
const tested: boolean = /x/.test("x");
const wrongTested: string = /x/.test("x"); // error[TK2322]: Type 'boolean' is not assignable to type 'string'
