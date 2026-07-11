// Backlog 74 — a forward call sees the declared overload set, never the wider
// implementation signature. Cross-checked against tsc 6.0.3 --strict.

function overloadContainer(): void {
  const wrongForwardResult: "s" = choose(1); // error[TK2322]: Type '"n"' is not assignable to type '"s"'
  choose(true); // error[TK2769]: No overload matches this call

  function choose(value: number): "n";
  function choose(value: string): "s";
  function choose(value: number | string | boolean): "n" | "s" {
    return "n";
  }

  const wrongAfterResult: "s" = choose(1); // error[TK2322]: Type '"n"' is not assignable to type '"s"'
  choose(true); // error[TK2769]: No overload matches this call
}

ambientForward(true); // error[TK2769]: No overload matches this call

declare function ambientForward(value: number): number;
declare function ambientForward(value: string): string;

exportedForward("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

export function exportedForward(value: number): number {
  return value;
}
