// The second augmentation and consumer run after 00_augment.ts in this project.
export {};

interface WU5Shared {
  consumeModuleOnly: boolean;
}

declare global {
  interface WU5Shared {
    fromConsume: string;
  }

  namespace WU5GlobalSpace {
    interface FromConsume { value: string }
    interface Shared { fromConsume: string }
  }
}

declare const forwardConsumer: WU5GlobalConsumer;
const forwardAugment: number = forwardConsumer.shared.fromAugment;
const forwardConsume: string = forwardConsumer.shared.fromConsume;
const forwardWrong: boolean = forwardConsumer.shared.fromAugment; // error[TK2322]: Type 'number' is not assignable to type 'boolean'

const consumeLocalOk: WU5Shared = { consumeModuleOnly: true };
const consumeLocalLeak: WU5Shared = { consumeModuleOnly: true, fromConsume: "local" }; // error[TK2353]

declare const forwardNamespaceAugment: WU5GlobalSpace.FromAugment;
const forwardNamespaceWrong: boolean = forwardNamespaceAugment.value; // error[TK2322]: Type 'number' is not assignable to type 'boolean'
declare const forwardNamespaceShared: WU5GlobalSpace.Shared;
const forwardNamespaceSharedAugment: number = forwardNamespaceShared.fromAugment;
const forwardNamespaceSharedConsume: string = forwardNamespaceShared.fromConsume;
