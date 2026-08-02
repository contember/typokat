export {};

const fromString: number = b103AmbientOverload("value");
const fromNumber: string = b103AmbientOverload(1);
const wrongFromString: string = b103AmbientOverload("value"); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongFromNumber: number = b103AmbientOverload(1); // error[TK2322]: Type 'string' is not assignable to type 'number'
