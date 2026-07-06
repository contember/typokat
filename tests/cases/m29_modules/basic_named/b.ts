import { goodNumber, badString, takesNumber } from "./a";

const ok: number = goodNumber;
const bad: number = badString; // error[TK2322]: Type 'string' is not assignable to type 'number'
takesNumber("x"); // error[TK2345]: Type 'string' is not assignable to type 'number'
