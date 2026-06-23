// M0 — assignability of a literal to a primitive annotation (the walking skeleton).
// Shape: `const NAME: <primitive> = <literal>;`

const ok_num: number = 1;
const ok_str: string = "hello";
const ok_bool: boolean = true;

const bad_num: number = "hello"; // error[TK2322]: Type 'string' is not assignable to type 'number'
const bad_str: string = 42;      // error[TK2322]: Type 'number' is not assignable to type 'string'
const bad_bool: boolean = 0;     // error[TK2322]: Type 'number' is not assignable to type 'boolean'
const bad_num2: number = true;   // error[TK2322]: Type 'boolean' is not assignable to type 'number'
