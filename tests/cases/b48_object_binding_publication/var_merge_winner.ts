// Object-binding and ordinary `var` declarations share the same visible winner storage.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit` reports TS2322 on lines 6, 9, 13, and 16.

var { b48ObjectFirstLeaf } = { b48ObjectFirstLeaf: "ready" };
const b48ObjectFirstBefore: string = b48ObjectFirstLeaf;
const b48ObjectFirstBeforeWrong: number = b48ObjectFirstLeaf; // error[TK2322]
var b48ObjectFirstLeaf = "still-ready";
const b48ObjectFirstAfter: string = b48ObjectFirstLeaf;
const b48ObjectFirstAfterWrong: number = b48ObjectFirstLeaf; // error[TK2322]

var b48OrdinaryFirstLeaf = 1;
const b48OrdinaryFirstBefore: number = b48OrdinaryFirstLeaf;
const b48OrdinaryFirstBeforeWrong: string = b48OrdinaryFirstLeaf; // error[TK2322]
var { b48OrdinaryFirstLeaf } = { b48OrdinaryFirstLeaf: 2 };
const b48OrdinaryFirstAfter: number = b48OrdinaryFirstLeaf;
const b48OrdinaryFirstAfterWrong: string = b48OrdinaryFirstLeaf; // error[TK2322]
