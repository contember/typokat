// M30 — contextual fresh object literals still get excess-property checks.

type Shape = { kind: "circle"; radius: number };

const decl: Shape = ({ kind: "circle", radius: 1, extra: 1 }); // error[TK2353]
const wrongKnownAndExcess: Shape = ({ kind: "square", radius: 1, extra: 1 }); // error[TK2322] | error[TK2353]

let s: Shape;
s = ({ kind: "circle", radius: 1, extra: 1 }); // error[TK2353]

declare function takes(shape: Shape): void;
takes(({ kind: "circle", radius: 1, extra: 1 })); // error[TK2353]

class Box {
  shape: Shape;
  constructor(shape: Shape) {
    this.shape = shape;
  }
}
new Box(({ kind: "circle", radius: 1, extra: 1 })); // error[TK2353]

class Base {
  constructor(shape: Shape) {}
}
class Child extends Base {
  constructor() {
    super(({ kind: "circle", radius: 1, extra: 1 })); // error[TK2353]
  }
}

function f(): Shape {
  return ({ kind: "circle", radius: 1, extra: 1 }); // error[TK2353]
}
const g = (): Shape => ({ kind: "circle", radius: 1, extra: 1 }); // error[TK2353]

const obj: { shape: Shape } = { shape: { kind: "circle", radius: 1 } };
obj.shape = ({ kind: "circle", radius: 1, extra: 1 }); // error[TK2353]

type TupleShape = [Shape];

const arrDecl: Shape[] = [{ kind: "circle", radius: 1, extra: 1 }]; // error[TK2353]
const tupleDecl: TupleShape = [{ kind: "circle", radius: 1, extra: 1 }]; // error[TK2353]

declare function takesArr(xs: Shape[]): void;
declare function takesTuple(xs: TupleShape): void;
takesArr([{ kind: "circle", radius: 1, extra: 1 }]); // error[TK2353]
takesTuple([{ kind: "circle", radius: 1, extra: 1 }]); // error[TK2353]

function retArr(): Shape[] {
  return [{ kind: "circle", radius: 1, extra: 1 }]; // error[TK2353]
}
const arrowArr = (): Shape[] => [{ kind: "circle", radius: 1, extra: 1 }]; // error[TK2353]

let arr: Shape[] = [];
arr = [{ kind: "circle", radius: 1, extra: 1 }]; // error[TK2353]

const holder: { arr: Shape[]; tuple: TupleShape } = { arr: [], tuple: [{ kind: "circle", radius: 1 }] };
holder.arr = [{ kind: "circle", radius: 1, extra: 1 }]; // error[TK2353]
holder.tuple = [{ kind: "circle", radius: 1, extra: 1 }]; // error[TK2353]

const emptyTarget: {} = { x: 1, y: 1 };
const emptyNestedTuple: [number, {}] = [5, { x: 1, y: 1 }];

type OptObj = { maybe?: Shape };
type UnionObj = { maybe: Shape | undefined };
type OptArr = { maybe?: Shape[] };
type UnionArr = { maybe: Shape[] | undefined };
type OptTuple = { maybe?: [Shape] };
type UnionTuple = { maybe: [Shape] | undefined };

const optObj: OptObj = { maybe: { kind: "circle", radius: 1, extra: 1 } }; // error[TK2353]
const unionObj: UnionObj = { maybe: { kind: "circle", radius: 1, extra: 1 } }; // error[TK2353]
const optArr: OptArr = { maybe: [{ kind: "circle", radius: 1, extra: 1 }] }; // error[TK2353]
const unionArr: UnionArr = { maybe: [{ kind: "circle", radius: 1, extra: 1 }] }; // error[TK2353]
const optTuple: OptTuple = { maybe: [{ kind: "circle", radius: 1, extra: 1 }] }; // error[TK2353]
const unionTuple: UnionTuple = { maybe: [{ kind: "circle", radius: 1, extra: 1 }] }; // error[TK2353]
