// Surface-accounting CONTROL (sprint 2026-07-10 WU5). Supported annotation forms must
// stay verdict-stable and clean: WU5 must NOT spuriously emit an incomplete record on
// keyof / mapped / conditional / template / readonly / tuple-rest. A file with no marker
// must check clean (0 diagnostics, 0 incomplete). See tests/cases/README.md.

type Obj = { a: number; b: string };
type Keys = keyof Obj;
type Part = { [K in keyof Obj]?: Obj[K] };
type Cond<T> = T extends string ? 1 : 2;
type IsStr = Cond<string>;
type Tmpl = `x${Keys}`;
type RO = readonly number[];

let k: Keys = "a";
let p: Part = {};
let one: IsStr = 1;
