// tsc 6.0.3 --strict --target es2025 --module commonjs: clean. Routing must select
// private compilation even while backlog 15 retains the UMD publication semantics.
export as namespace B14Umd; // incomplete[decl/namespace-export/self]
export = B14Umd; // incomplete[decl/export-assignment/self]

declare function B14Umd(value: number): number;
declare namespace B14Umd {
  const version: string;
  interface Options {
    enabled: boolean;
  }
}
