// F1 / backlog 05 (WU1) - object type literal method signatures lower to
// function-typed properties, while ordinary properties still coexist with them.
// Cross-checked against tsc 6.0.3 --strict.

type ObjectService = {
  enabled: boolean;
  convert(input: number): string;
};

declare const objectService: ObjectService;

const objectMethod: (value: number) => string = objectService.convert; // ok - method member reads as a function
const objectResult: string = objectService.convert(1);                 // ok
const objectBadResult: boolean = objectService.convert(1);             // error[TK2322]: Type 'string' is not assignable to type 'boolean'
objectService.missing;                                                 // error[TK2339]: Property 'missing' does not exist
objectService.convert("bad");                                          // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

const objectProp: boolean = objectService.enabled;                     // ok - plain property still lowers
const objectBadProp: string = objectService.enabled;                   // error[TK2322]: Type 'boolean' is not assignable to type 'string'
