// WU6A project oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs
// 00_consume.ts 99_reopen.ts. The later reopening is visible here; only TS2322 is expected.

namespace Wu6aProjectOrder {
  export const first: number = 1;
  export interface Shape {
    first: number;
    second: string;
  }
}

const wu6aProjectForwardAlias = Wu6aProjectOrder;
const wu6aProjectForwardFirst: number = wu6aProjectForwardAlias.first;
const wu6aProjectForwardSecond: string = wu6aProjectForwardAlias.second;
const wu6aProjectForwardNested: number = Wu6aProjectOrder.Nested.count;
const wu6aProjectForwardType: Wu6aProjectOrder.Shape = { first: 1, second: "two" };
const wu6aProjectForwardWrong: number = Wu6aProjectOrder.second; // error[TK2322]: Type 'string' is not assignable to type 'number'
