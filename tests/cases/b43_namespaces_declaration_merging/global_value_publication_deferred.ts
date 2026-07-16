// tsc 6.0.3 --strict: TS2322 and TS2339 on the two demands below.
export {};

namespace DeferredRoot {
  export interface DeferredLeaf { moduleOnly: number }
}

declare global { // incomplete[decl/global-declaration/self]: global augmentation value publication not modeled
  const WU5GlobalConst: { value: number };
  function wu5GlobalFunction(value: number): string;
  class WU5GlobalClass { value: number }
  namespace DeferredRoot {
    export class DeferredLeaf { globalOnly: string }
  }
  interface WU5DeferredCarrier { value: DeferredRoot.DeferredLeaf }
}

// tsc sees the withheld global class here: TS2322 for globalOnly and TS2339 for moduleOnly.
// typokat withholds the dependent carrier under the backlog-82 incomplete record above.
declare const deferredCarrier: WU5DeferredCarrier;
const deferredGlobalWrong: number = deferredCarrier.value.globalOnly; // error[TK2339]
const deferredModuleLeak: number = deferredCarrier.value.moduleOnly; // error[TK2339]
