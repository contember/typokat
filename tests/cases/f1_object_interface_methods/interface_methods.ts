// F1 / backlog 05 (WU1) - interface method signatures lower to
// function-typed properties, while ordinary properties still coexist with them.
// Cross-checked against tsc 6.0.3 --strict.

interface InterfaceService {
  value: string;
  convert(input: number): string;
}

declare const interfaceService: InterfaceService;

const interfaceMethod: (value: number) => string = interfaceService.convert; // ok - parameter names do not affect assignability
const interfaceResult: string = interfaceService.convert(1);                 // ok
const interfaceBadResult: number = interfaceService.convert(1);              // error[TK2322]: Type 'string' is not assignable to type 'number'
interfaceService.missing;                                                    // error[TK2339]: Property 'missing' does not exist
interfaceService.convert("bad");                                             // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

const interfaceProp: string = interfaceService.value;                        // ok - plain property still lowers
const interfaceBadProp: number = interfaceService.value;                     // error[TK2322]: Type 'string' is not assignable to type 'number'

interface InterfaceShapedMethods {
  optional(input: number, label?: string): void;
  rest(input: number, ...labels: string[]): void;
}

declare const interfaceShapedMethods: InterfaceShapedMethods;
interfaceShapedMethods.optional(1);
interfaceShapedMethods.optional();                                           // error[TK2554]: Expected 1-2 arguments, but got 0
interfaceShapedMethods.optional(1, 2);                                       // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
interfaceShapedMethods.rest(1, "x");
interfaceShapedMethods.rest();                                               // error[TK2555]: Expected at least 1 arguments, but got 0
interfaceShapedMethods.rest(1, 2);                                           // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
