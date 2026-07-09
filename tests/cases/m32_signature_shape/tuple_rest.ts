// M32 - tuple rest elements, including readonly tuple rest. Cross-checked with
// tsc 6.0.3 --strict. Alias/tuple-target mismatches use code-only markers where
// the rendered target shape is not stable enough to pin.

type Resty = [string, ...number[]];
const t1: Resty = ["x"];
const t2: Resty = ["x", 1, 2];
const t3: Resty = [1]; // error[TK2322]: Type 'number' is not assignable to type 'string'
const t4: Resty = ["x", "no"]; // error[TK2322]

type WithTail<T extends unknown[]> = [...T, boolean];
const v1: WithTail<[string, number]> = ["x", 1, true];
const v2: WithTail<[string, number]> = ["x", true]; // error[TK2322]

type Middle = [string, ...number[], boolean];
const m1: Middle = ["x", true];
const m2: Middle = ["x", 1, 2, true];
const m3: Middle = ["x", "no", true]; // error[TK2322]

type ReadonlyRest = readonly [string, ...number[]];
const rr1: ReadonlyRest = ["x", 1];
const rr2: ReadonlyRest = ["x", "no"]; // error[TK2322]
