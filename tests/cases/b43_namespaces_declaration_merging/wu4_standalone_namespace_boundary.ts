// WU4 — tsc 6.0.3 --strict --noEmit --lib es5 --module commonjs: TS2322 below.
// The exported value keeps the standalone runtime surface explicitly incomplete;
// this fixture makes no claim that the namespace publishes a value receiver.
namespace Wu4StandaloneNamespace { // incomplete[decl/module-declaration/self]
  export const deferredValue: number = 1;
  export interface Item {
    value: number;
  }
  export type Alias = { label: string };
}

const wu4StandaloneItem: Wu4StandaloneNamespace.Item = { value: 1 };
const wu4StandaloneAlias: Wu4StandaloneNamespace.Alias = { label: "ok" };
const wu4StandaloneWrong: number = wu4StandaloneAlias.label; // error[TK2322]: Type 'string' is not assignable to type 'number'
