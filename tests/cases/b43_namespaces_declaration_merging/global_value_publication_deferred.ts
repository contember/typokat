// tsc 6.0.3 --strict: four TS2322 assignments and one TS2339 namespace leak below.
export {};

namespace DeferredRoot {
  export interface DeferredLeaf { moduleOnly: number }
}

declare global {
  const WU5GlobalConst: { value: number };
  function wu5GlobalFunction(value: number): string;
  class WU5GlobalClass { value: number }
  namespace DeferredRoot {
    export class DeferredLeaf { globalOnly: string }
  }
  interface WU5DeferredCarrier { value: DeferredRoot.DeferredLeaf }
}

const globalConstGood: number = WU5GlobalConst.value;
const globalConstWrong: string = WU5GlobalConst.value; // error[TK2322]
const globalFunctionGood: string = wu5GlobalFunction(1);
const globalFunctionWrong: number = wu5GlobalFunction(1); // error[TK2322]
const globalClassGood: number = new WU5GlobalClass().value;
const globalClassWrong: string = new WU5GlobalClass().value; // error[TK2322]

// The global class augments the global namespace, not the module-local namespace above.
declare const deferredCarrier: WU5DeferredCarrier;
const deferredGlobalWrong: number = deferredCarrier.value.globalOnly; // error[TK2322]
const deferredModuleLeak: number = deferredCarrier.value.moduleOnly; // error[TK2339]
