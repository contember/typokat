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

class MutualClassHeritageA { // error[TK2310]: recursively references itself as a base type
  ownA: number = 1;
}
interface MutualClassHeritageA extends MutualClassHeritageB {} // error[TK2310]: recursively references itself as a base type
class MutualClassHeritageB { // error[TK2310]: recursively references itself as a base type
  ownB: string = "b";
}
interface MutualClassHeritageB extends MutualClassHeritageA {} // error[TK2310]: recursively references itself as a base type

const mutualClassHeritageAOwn: number = new MutualClassHeritageA().ownA;
const mutualClassHeritageAOwnWrong: string = new MutualClassHeritageA().ownA; // error[TK2322]: Type 'number' is not assignable to type 'string'
new MutualClassHeritageA().ownB; // error[TK2339]: Property 'ownB' does not exist on type 'MutualClassHeritageA'
const mutualClassHeritageBOwn: string = new MutualClassHeritageB().ownB;
const mutualClassHeritageBInherited: number = new MutualClassHeritageB().ownA;
const mutualClassHeritageBOwnWrong: number = new MutualClassHeritageB().ownB; // error[TK2322]: Type 'string' is not assignable to type 'number'
const mutualClassHeritageBInheritedWrong: string = new MutualClassHeritageB().ownA; // error[TK2322]: Type 'number' is not assignable to type 'string'

class MixedHardSoftCycleB { // error[TK2310]: recursively references itself as a base type
  b: string = "b";
}
interface MixedHardSoftCycleB extends MixedHardSoftCycleA {} // error[TK2310]: recursively references itself as a base type
class MixedHardSoftCycleA extends MixedHardSoftCycleB { // error[TK2310]: recursively references itself as a base type
  a: number = 1;
}

const mixedHardSoftCycleAOwn: number = new MixedHardSoftCycleA().a;
const mixedHardSoftCycleAOwnWrong: string = new MixedHardSoftCycleA().a; // error[TK2322]: Type 'number' is not assignable to type 'string'
const mixedHardSoftCycleBOwn: string = new MixedHardSoftCycleB().b;
const mixedHardSoftCycleBOwnWrong: number = new MixedHardSoftCycleB().b; // error[TK2322]: Type 'string' is not assignable to type 'number'
