// The first augmentation's source now runs after 00_consume.ts in this project.
export {};

interface WU5Shared {
  augmentModuleOnly: boolean;
}

declare global {
  interface WU5Shared {
    fromAugment: number;
  }

  interface WU5GlobalConsumer {
    shared: WU5Shared;
  }
}

const augmentLocalOk: WU5Shared = { augmentModuleOnly: true };
const augmentLocalLeak: WU5Shared = { augmentModuleOnly: true, fromAugment: 1 }; // error[TK2353]
