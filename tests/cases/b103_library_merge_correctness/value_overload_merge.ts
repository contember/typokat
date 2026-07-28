// Backlog 103 correctness: a legal user overload is appended to the library function group.
declare function parseInt(value: "one"): 1;

const one: 1 = parseInt("one");
const ordinary: number = parseInt("10");
const wrongOrdinary: string = parseInt("10"); // error[TK2322]: Type 'number' is not assignable to type 'string'
