// Backlog 70 — a generic bind-shaped declaration substitutes the receiver slot
// independently from positional/rest arguments and the return type.

declare function bind<T, A extends unknown[], R>(
  fn: (this: T, ...args: A) => R,
  receiver: T,
): (...args: A) => R;

function render(this: { n: number }, value: string): number {
  return this.n;
}

function wrongReceiver(this: { n: string }, value: string): number {
  return value.length;
}

const bound = bind(render, { n: 1 });
const boundOk: number = bound("ok");
bound(1); // error[TK2345]

bind<{ n: number }, [string], number>(wrongReceiver, { n: 1 }); // error[TK2345]
