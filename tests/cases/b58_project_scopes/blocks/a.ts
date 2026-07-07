// b58: keep this header byte-identical with b.ts so both blocks align.
{
  const na: string = "x";
  const ua: number = na; // error[TK2322]: Type 'string' is not assignable to type 'number'
}
export const za = 1;
