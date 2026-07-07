// backlog 61 control: every field initializer here is legal — the new checks
// must not over-report on valid instance/static/readonly/inferred fields.
type Shape = { kind: string };
class Ok {
  n: number = 1;
  s: Shape = { kind: "c" };
  t: [number, string] = [1, "x"];
  u: string | number = 2;
  arr: number[] = [1, 2];
  readonly r: number = 1;
  inferred = "s";
  static sok: string = "fine";
  opt1?: number = undefined;
  opt2?: number = 3;
}
const k = new Ok();
const readBack: string = k.inferred;
