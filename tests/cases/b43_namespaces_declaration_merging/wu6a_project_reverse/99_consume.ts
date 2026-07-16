// WU6A project oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs
// 00_reopen.ts 99_consume.ts. The earlier reopening has the same surface; only TS2322 is expected.

namespace Wu6aProjectOrder {
  export const first: number = 1;
  export interface Shape {
    first: number;
    second: string;
  }
}

const wu6aProjectReverseAlias = Wu6aProjectOrder;
const wu6aProjectReverseFirst: number = wu6aProjectReverseAlias.first;
const wu6aProjectReverseSecond: string = wu6aProjectReverseAlias.second;
const wu6aProjectReverseNested: number = Wu6aProjectOrder.Nested.count;
const wu6aProjectReverseType: Wu6aProjectOrder.Shape = { first: 1, second: "two" };
const wu6aProjectReverseWrong: number = Wu6aProjectOrder.second; // error[TK2322]: Type 'string' is not assignable to type 'number'
