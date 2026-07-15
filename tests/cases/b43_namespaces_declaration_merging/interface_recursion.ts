// tsc 6.0.3 --strict: TS2741 x3 and TS2322 x6 below; recursive merge groups are otherwise clean.
interface RecursiveNode {
  next?: RecursiveNode;
}
interface RecursiveNode {
  value: number;
}
const recursiveMissing: RecursiveNode = { next: { value: 1 } }; // error[TK2741]
declare const recursiveDemand: RecursiveNode;
const recursiveValue: number = recursiveDemand.value;
const recursiveWrongLeaf: string = recursiveDemand.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

interface ForwardLeft {
  right?: ForwardRight;
}
interface ForwardLeft {
  leftValue: number;
}
interface ForwardRight {
  left?: ForwardLeft;
}
interface ForwardRight {
  rightValue: string;
}
const forwardMissing: ForwardLeft = {}; // error[TK2741]
declare const forwardDemand: ForwardLeft;
const forwardWrongLeaf: string = forwardDemand.leftValue; // error[TK2322]: Type 'number' is not assignable to type 'string'

interface ReverseRight {
  rightValue: string;
}
interface ReverseRight {
  left?: ReverseLeft;
}
interface ReverseLeft {
  leftValue: number;
}
interface ReverseLeft {
  right?: ReverseRight;
}
const reverseMissing: ReverseRight = {}; // error[TK2741]
declare const reverseDemand: ReverseRight;
const reverseRightValue: string = reverseDemand.rightValue;
const reverseWrongLeaf: number = reverseDemand.rightValue; // error[TK2322]: Type 'string' is not assignable to type 'number'

interface GenericRecursive<T> {
  next: GenericRecursive<T>;
}
interface GenericRecursive<T> {
  value: T;
}
declare const genericRecursive: GenericRecursive<number>;
const genericEdgeValue: number = genericRecursive.next.next.value;
const genericEdgeWrong: string = genericRecursive.next.next.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

interface EdgeLeft {
  right: EdgeRight;
}
interface EdgeLeft {
  value: number;
}
interface EdgeRight {
  left: EdgeLeft;
}
interface EdgeRight {
  label: string;
}
declare const edgeLeft: EdgeLeft;
declare const edgeRight: EdgeRight;
const leftThroughMutualWrong: string = edgeLeft.right.left.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const leftThroughMutual: number = edgeLeft.right.left.value;
const rightThroughMutual: string = edgeRight.left.right.label;
const rightThroughMutualWrong: number = edgeRight.left.right.label; // error[TK2322]: Type 'string' is not assignable to type 'number'
