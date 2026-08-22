// Project-local library-index sprint: callable namespace surfaces must lower the
// ordinary `symbol` keyword used by Reflect.ownKeys.
declare namespace SymbolNamespaceSurface {
  function ownKeys(target: object): (string | symbol)[];
  function echo(value: symbol): symbol;
}

const keys: (string | symbol)[] = SymbolNamespaceSurface.ownKeys({ value: 1 });
const wrongKeys: number[] = SymbolNamespaceSurface.ownKeys({ value: 1 }); // error[TK2322]

declare const sourceSymbol: symbol;
const symbolValue: symbol = SymbolNamespaceSurface.echo(sourceSymbol);
SymbolNamespaceSurface.echo("not a symbol"); // error[TK2345]
