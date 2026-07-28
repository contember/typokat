const value: number = B103Umd(1);
const wrongValue: string = B103Umd(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const version: string = B103Umd.version;
const wrongVersion: number = B103Umd.version; // error[TK2322]: Type 'string' is not assignable to type 'number'
declare const options: B103Umd.Options;
const enabled: boolean = options.enabled;
const wrongEnabled: string = options.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'string'
B103Umd.missing; // error[TK2339]
