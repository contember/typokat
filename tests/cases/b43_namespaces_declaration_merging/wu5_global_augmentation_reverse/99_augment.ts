// The first augmentation's source now runs after 00_consume.ts in this project.
export {};

interface WU5Shared {
  augmentModuleOnly: boolean;
}

namespace WU5GlobalSpace {
  export interface ModuleOnly { augmentModuleNamespaceOnly: boolean }
}

declare global {
  interface WU5Shared {
    fromAugment: number;
  }

  interface WU5GlobalConsumer {
    shared: WU5Shared;
  }

  namespace WU5GlobalSpace {
    interface FromAugment { value: number }
    interface Shared { fromAugment: number }
  }
}

const augmentLocalOk: WU5Shared = { augmentModuleOnly: true };
const augmentLocalLeak: WU5Shared = { augmentModuleOnly: true, fromAugment: 1 }; // error[TK2353]
const augmentNamespaceLocalOk: WU5GlobalSpace.ModuleOnly = { augmentModuleNamespaceOnly: true };
type AugmentNamespaceGlobalLeak = WU5GlobalSpace.FromAugment; // error[TK2694]
