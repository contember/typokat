declare const key: string;
declare const rootValue: number;

const boundControl = () => rootValue;
const skippedConditional = true ? (() => rootValue) : (() => rootValue);
const skippedKey = { [(() => key)()]: 1 };
const skippedComputedMember = values[(() => key)()];

@decorate(() => rootValue)
class Holder {
  field = () => rootValue;
  method() { return rootValue; }
  [(() => key)()]() { return rootValue; }
  static { const skippedStatic = () => rootValue; }
}

type ComputedType = { [(() => key)()]: number };
type FunctionType = <T>(value: T) => T;
