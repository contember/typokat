interface C01<T extends Missing> { value: T } // error[TK2428]: All declarations of 'C01' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
interface C01<T extends string> {} // error[TK2428]: All declarations of 'C01' must have identical type parameters.
declare const c01: C01<number>;
const c01Recovery: number = c01.value;
interface C02<T extends string> { value: T } // error[TK2428]: All declarations of 'C02' must have identical type parameters.
interface C02<T extends Missing> {} // error[TK2428]: All declarations of 'C02' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
declare const c02: C02<number>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
const c02Recovery: number = c02.value;
interface C03<T extends MissingA> { value: T } // error[TK2304]: Cannot find name 'MissingA'
interface C03<T extends MissingB> {} // error[TK2304]: Cannot find name 'MissingB'
declare const c03: C03<number>;
interface C04<T extends Missing> { value: T } // error[TK2304]: Cannot find name 'Missing'
interface C04<T extends Missing> {} // error[TK2304]: Cannot find name 'Missing'
declare const c04: C04<number>;
interface C05<T extends Missing> { value: T } // error[TK2304]: Cannot find name 'Missing'
interface C05<T> {}
declare const c05: C05<number>;
interface C06<T> { value: T }
interface C06<T extends Missing> {} // error[TK2304]: Cannot find name 'Missing'
declare const c06: C06<number>;
interface C07<T extends MissingA> { value: T } // error[TK2428]: All declarations of 'C07' must have identical type parameters. | error[TK2304]: Cannot find name 'MissingA'
interface C07<T extends string> {} // error[TK2428]: All declarations of 'C07' must have identical type parameters.
interface C07<T extends MissingB> {} // error[TK2428]: All declarations of 'C07' must have identical type parameters. | error[TK2304]: Cannot find name 'MissingB'
declare const c07: C07<number>;
interface C08<T, U extends Missing> { value: U } // error[TK2428]: All declarations of 'C08' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
interface C08<T, U extends T> {} // error[TK2428]: All declarations of 'C08' must have identical type parameters.
declare const c08: C08<string, number>;
const c08Recovery: number = c08.value;
interface C09<T, U extends T> { value: U } // error[TK2428]: All declarations of 'C09' must have identical type parameters.
interface C09<T, U extends Missing> {} // error[TK2428]: All declarations of 'C09' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
declare const c09: C09<string, number>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
const c09Recovery: number = c09.value;
interface D01<T = Missing> { value: T } // error[TK2428]: All declarations of 'D01' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
interface D01<T = string> {} // error[TK2428]: All declarations of 'D01' must have identical type parameters.
declare const d01: D01; // incomplete[annotation-lower/type-reference/default-argument]
const d01Recovery: number = d01.value;
interface D02<T = string> { value: T } // error[TK2428]: All declarations of 'D02' must have identical type parameters.
interface D02<T = Missing> {} // error[TK2428]: All declarations of 'D02' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
declare const d02: D02;
const d02Recovery: number = d02.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
interface D03<T = MissingA> { value: T } // error[TK2304]: Cannot find name 'MissingA'
interface D03<T = MissingB> {} // error[TK2304]: Cannot find name 'MissingB'
declare const d03: D03; // incomplete[annotation-lower/type-reference/default-argument]
interface D04<T = Missing> { value: T } // error[TK2304]: Cannot find name 'Missing'
interface D04<T = Missing> {} // error[TK2304]: Cannot find name 'Missing'
declare const d04: D04; // incomplete[annotation-lower/type-reference/default-argument]
interface D05<T = Missing> { value: T } // error[TK2304]: Cannot find name 'Missing'
interface D05<T> {}
declare const d05: D05; // incomplete[annotation-lower/type-reference/default-argument]
interface D06<T> { value: T }
interface D06<T = Missing> {} // error[TK2304]: Cannot find name 'Missing'
declare const d06: D06; // incomplete[annotation-lower/type-reference/default-argument]
interface D07<T = MissingA> { value: T } // error[TK2428]: All declarations of 'D07' must have identical type parameters. | error[TK2304]: Cannot find name 'MissingA'
interface D07<T = string> {} // error[TK2428]: All declarations of 'D07' must have identical type parameters.
interface D07<T = MissingB> {} // error[TK2428]: All declarations of 'D07' must have identical type parameters. | error[TK2304]: Cannot find name 'MissingB'
declare const d07: D07; // incomplete[annotation-lower/type-reference/default-argument]
interface D08<T = string, U = Missing> { value: U } // error[TK2428]: All declarations of 'D08' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
interface D08<T, U = T> {} // error[TK2428]: All declarations of 'D08' must have identical type parameters.
declare const d08: D08; // incomplete[annotation-lower/type-reference/default-argument]
interface D09<T = string, U = T> { value: U } // error[TK2428]: All declarations of 'D09' must have identical type parameters.
interface D09<T, U = Missing> {} // error[TK2428]: All declarations of 'D09' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
declare const d09: D09;
const d09Recovery: number = d09.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
interface D10<T = Missing, U = T> { value: U } // error[TK2428]: All declarations of 'D10' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
interface D10<T = string, U> {} // error[TK2428]: All declarations of 'D10' must have identical type parameters. | error[TK2706]: Required type parameters may not follow optional type parameters
declare const d10: D10; // incomplete[annotation-lower/type-reference/default-argument]
interface X01<T extends Missing = number> { value: T } // error[TK2428]: All declarations of 'X01' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing'
interface X01<T extends string> {} // error[TK2428]: All declarations of 'X01' must have identical type parameters.
declare const x01: X01;
declare const x01Arg: X01<number>;
interface X02<T extends string> { value: T } // error[TK2428]: All declarations of 'X02' must have identical type parameters.
interface X02<T extends Missing = number> {} // error[TK2428]: All declarations of 'X02' must have identical type parameters. | error[TK2304]: Cannot find name 'Missing' | error[TK2344]: Type 'number' does not satisfy the constraint 'string'
declare const x02: X02;
declare const x02Arg: X02<number>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
interface P01<T = T> {} // error[TK2428]: All declarations of 'P01' must have identical type parameters. | error[TK2744]: Type parameter defaults can only reference previously declared type parameters
interface P01<T = string> {} // error[TK2428]: All declarations of 'P01' must have identical type parameters.
interface P02<T = string> {} // error[TK2428]: All declarations of 'P02' must have identical type parameters.
interface P02<T = T> {} // error[TK2428]: All declarations of 'P02' must have identical type parameters. | error[TK2744]: Type parameter defaults can only reference previously declared type parameters
interface P03<T = T> {} // error[TK2744]: Type parameter defaults can only reference previously declared type parameters
interface P03<T = T> {} // error[TK2744]: Type parameter defaults can only reference previously declared type parameters
interface P04<T = U, U = string> {} // error[TK2428]: All declarations of 'P04' must have identical type parameters. | error[TK2744]: Type parameter defaults can only reference previously declared type parameters
interface P04<T = string, U = string> {} // error[TK2428]: All declarations of 'P04' must have identical type parameters.
interface P05<T = string, U = string> {} // error[TK2428]: All declarations of 'P05' must have identical type parameters.
interface P05<T = U, U = string> {} // error[TK2428]: All declarations of 'P05' must have identical type parameters. | error[TK2744]: Type parameter defaults can only reference previously declared type parameters
