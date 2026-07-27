// The consumer half; see 00_declare.ts.
const crossLabel: string = b103CrossValue.label;
const wrongCrossLabel: number = b103CrossValue.label; // error[TK2322]: Type 'string' is not assignable to type 'number'
