// tsc 6.0.3 --strict --module commonjs: clean; UMD namespace export stays declaration-only.
export as namespace WU0Library; // incomplete[decl/namespace-export/self]
export = WU0Library; // incomplete[decl/export-assignment/self]

declare function WU0Library(value: number): number;
declare namespace WU0Library {
  interface Options { enabled: boolean }
}

declare const umdOptions: WU0Library.Options;
