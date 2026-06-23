// M8 — `in`-operator narrowing: keep the union members that have the property.

type Box = { a: number } | { b: string };

function f(x: Box) {
  const bad = x.a; // error[TK2339]: Property 'a' does not exist
  if ("a" in x) {
    const ok: number = x.a; // ok — narrowed to { a: number }
  } else {
    const ok2: string = x.b; // ok — complement: { b: string }
  }
}
