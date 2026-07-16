// WU5 project oracle — same declarations with the input order reversed.
export {};

interface WU5Shared {
  consumeModuleOnly: boolean;
}

declare global {
  interface WU5Shared {
    fromConsume: string;
  }
}

declare const reverseConsumer: WU5GlobalConsumer;
const reverseAugment: number = reverseConsumer.shared.fromAugment;
const reverseConsume: string = reverseConsumer.shared.fromConsume;
const reverseWrong: boolean = reverseConsumer.shared.fromAugment; // error[TK2322]: Type 'number' is not assignable to type 'boolean'

const consumeLocalOk: WU5Shared = { consumeModuleOnly: true };
const consumeLocalLeak: WU5Shared = { consumeModuleOnly: true, fromConsume: "local" }; // error[TK2353]
