import { includedValue } from "./value";

const admittedExtensionlessWrong: string = includedValue; // error[TK2322]: Type 'number' is not assignable to type 'string'
