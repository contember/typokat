// WU0 addendum — mutual class/interface recursion works in both declaration orders.
class RecursiveClassFirst {
  ownLeft = 1;
}
interface RecursiveClassFirst {
  addedLeft: string;
  right: RecursiveInterfaceFirst;
}

interface RecursiveInterfaceFirst {
  addedRight: number;
  left: RecursiveClassFirst;
}
class RecursiveInterfaceFirst {
  ownRight = "right";
}

declare const recursiveLeft: RecursiveClassFirst;
declare const recursiveRight: RecursiveInterfaceFirst;
const leftEdge: string = recursiveLeft.right.left.addedLeft;
const leftEdgeWrong: number = recursiveLeft.right.left.addedLeft; // error[TK2322]: Type 'string' is not assignable to type 'number'
const rightEdge: number = recursiveRight.left.right.addedRight;
const rightEdgeWrong: string = recursiveRight.left.right.addedRight; // error[TK2322]: Type 'number' is not assignable to type 'string'

const recursiveLeftMissing: RecursiveClassFirst = { // error[TK2741]
  ownLeft: 1,
  right: recursiveRight,
};
const recursiveRightMissing: RecursiveInterfaceFirst = { // error[TK2741]
  ownRight: "right",
  left: recursiveLeft,
};
