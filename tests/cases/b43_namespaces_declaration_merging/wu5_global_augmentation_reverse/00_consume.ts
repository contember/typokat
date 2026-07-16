// WU5 project oracle — same declarations with the input order reversed.
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

declare const reverseConsumer: WU5GlobalConsumer;
const reverseAugment: number = reverseConsumer.shared.fromAugment;
const reverseConsume: string = reverseConsumer.shared.fromConsume;
const reverseWrong: boolean = reverseConsumer.shared.fromAugment; // error[TK2322]: Type 'number' is not assignable to type 'boolean'

const consumeLocalOk: WU5Shared = { consumeModuleOnly: true };
const consumeLocalLeak: WU5Shared = { consumeModuleOnly: true, fromConsume: "local" }; // error[TK2353]

declare const reverseNamespaceAugment: WU5GlobalSpace.FromAugment;
const reverseNamespaceWrong: boolean = reverseNamespaceAugment.value; // error[TK2322]: Type 'number' is not assignable to type 'boolean'
declare const reverseNamespaceShared: WU5GlobalSpace.Shared;
const reverseNamespaceSharedAugment: number = reverseNamespaceShared.fromAugment;
const reverseNamespaceSharedConsume: string = reverseNamespaceShared.fromConsume;

type ReverseModuleLocalLeak = WU5AugmentLocalShape; // error[TK2304]: Cannot find name 'WU5AugmentLocalShape'
declare const reverseLocalCapture: WU5GlobalLocalCapture;
const reverseCaptured: number = reverseLocalCapture.value.captured;
const reverseCapturedWrong: boolean = reverseLocalCapture.value.captured; // error[TK2322]: Type 'number' is not assignable to type 'boolean'
