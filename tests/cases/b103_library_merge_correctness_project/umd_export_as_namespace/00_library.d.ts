export as namespace B103Umd; // incomplete[decl/namespace-export/self]
export = B103Umd; // incomplete[decl/export-assignment/self]

declare function B103Umd(value: number): number;
declare namespace B103Umd {
  const version: string;
  interface Options {
    enabled: boolean;
  }
}
