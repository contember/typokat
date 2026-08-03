// tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs:
// TS2322 only on the four marked number/boolean sources.

interface Object {
  [key: number]: Object;
}

declare const wu7ObjectNever: never;
const wu7ObjectFromNever: Object = wu7ObjectNever;
const wu7ObjectEmptyArray: Object = [];
const wu7ObjectArray: Object = [{}];
const wu7NumberArray: Object = [1]; // error[TK2322]
const wu7ObjectEmptyTuple: Object = [] as [];
const wu7ObjectTuple: Object = [{}] as [{}];
const wu7NumberTuple: Object = [1] as [number]; // error[TK2322]
const wu7ReadonlyObjectArray: Object = [{}] as readonly Object[];
const wu7ReadonlyNumberArray: Object = [1] as readonly number[]; // error[TK2322]
const wu7ObjectString: Object = "x";
const wu7ObjectBoolean: Object = true; // error[TK2322]
