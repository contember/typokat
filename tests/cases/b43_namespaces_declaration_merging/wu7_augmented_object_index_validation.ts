// tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs:
// TS2411 on both marked properties and TS2322 on the primitive assignment;
// `{}` remains assignable to the augmented global Object.
// The same command also reports TS2411 for affected declarations inside lib.es5.d.ts.

class Wu7AugmentedObjectPayload {
  value!: string;
}

interface Object {
  wu7Payload: Wu7AugmentedObjectPayload; // error[TK2411]
  [key: string]: Object;
}

interface Wu7LocalObjectIndexValue {
  required: number;
}

interface Wu7LocalObjectIndexControl {
  payload: Wu7AugmentedObjectPayload; // error[TK2411]
  [key: string]: Wu7LocalObjectIndexValue;
}

const wu7AugmentedObjectTopControl: Object = {};
const wu7AugmentedObjectPrimitive: Object = 1; // error[TK2322]
