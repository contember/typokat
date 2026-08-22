const directValue: number = [1, 2].fromDirectScript();
const globalValues: readonly number[] = [1, 2].fromDeclareGlobal();
const mapped: number[] = [1, 2].map((value) => value + 1);
const wrongDirect: string = [1, 2].fromDirectScript(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongGlobal: string[] = [1, 2].fromDeclareGlobal(); // error[TK4104]: The type 'readonly number[]' is 'readonly' and cannot be assigned to the mutable type 'string[]'
