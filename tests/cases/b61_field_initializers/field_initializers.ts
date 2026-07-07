// backlog 61: class field initializers must be checked against the declared
// annotation exactly like variable-declaration initializers — assignability,
// fresh-literal excess, and M30 contextual typing; instance and static alike.
// At the buggy HEAD every marked line below is silent.
type Shape = { kind: string };
class C {
  n: number = "not-number"; // error[TK2322]: Type 'string' is not assignable to type 'number'
  s: Shape = { kind: "c", extra: 1 }; // error[TK2353]: 'extra' does not exist
  t: [number, string] = [1, 2]; // error[TK2322]: Type '[1, 2]' is not assignable to type '[number, string]'
  o: { a: { b: number } } = { a: { b: 1, c: 2 } }; // error[TK2353]: 'c' does not exist
  u: string | number = true; // error[TK2322]
  arr: number[] = [1, "x"]; // error[TK2322]
  ok: number = 1;
  readonly r: number = 1;
  inferred = "s";
  static sn: number = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
  static sok: string = "fine";
}
const c = new C();
const useInferred: number = c.inferred; // error[TK2322]: Type 'string' is not assignable to type 'number'
