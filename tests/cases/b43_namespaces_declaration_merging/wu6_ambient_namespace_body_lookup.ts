// tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs:
// TS2322 x7, TS2304, and TS2694. Ambient members are public by default.

declare namespace Wu6AmbientSameProviderFirst {
  interface Item { value: number }
  interface Box { item: Item }
}

declare const sameProviderFirst: Wu6AmbientSameProviderFirst.Box;
const sameProviderFirstValue: number = sameProviderFirst.item.value;
const sameProviderFirstWrong: string = sameProviderFirst.item.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare namespace Wu6AmbientSameConsumerFirst {
  interface Box { item: Item }
  interface Item { value: number }
}

declare const sameConsumerFirst: Wu6AmbientSameConsumerFirst.Box;
const sameConsumerFirstValue: number = sameConsumerFirst.item.value;
const sameConsumerFirstWrong: string = sameConsumerFirst.item.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare namespace Wu6AmbientReopenedProviderFirst {
  interface Item { value: number }
}

declare namespace Wu6AmbientReopenedProviderFirst {
  interface Box { item: Item }
}

declare const reopenedProviderFirst: Wu6AmbientReopenedProviderFirst.Box;
const reopenedProviderFirstValue: number = reopenedProviderFirst.item.value;
const reopenedProviderFirstWrong: string = reopenedProviderFirst.item.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare namespace Wu6AmbientReopenedConsumerFirst {
  interface Box { item: Item }
}

declare namespace Wu6AmbientReopenedConsumerFirst {
  interface Item { value: number }
}

declare const reopenedConsumerFirst: Wu6AmbientReopenedConsumerFirst.Box;
const reopenedConsumerFirstValue: number = reopenedConsumerFirst.item.value;
const reopenedConsumerFirstWrong: string = reopenedConsumerFirst.item.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

namespace Wu6PrivateFragmentControl {
  interface Local { value: number }
  interface FirstOnly { value: number }
  export interface Shared { value: number }
  export interface FromFirst { local: Local; first: FirstOnly }
}

namespace Wu6PrivateFragmentControl {
  interface Local { value: string }
  export interface FromSecond { local: Local; shared: Shared }
  export interface CannotSeeFirstPrivate {
    leaked: FirstOnly; // error[TK2304]: Cannot find name 'FirstOnly'
  }
}

declare const fromFirst: Wu6PrivateFragmentControl.FromFirst;
const firstLocalValue: number = fromFirst.local.value;
const firstLocalWrong: string = fromFirst.local.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const fromSecond: Wu6PrivateFragmentControl.FromSecond;
const secondLocalValue: string = fromSecond.local.value;
const secondLocalWrong: number = fromSecond.local.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const secondSharedValue: number = fromSecond.shared.value;
const secondSharedWrong: string = fromSecond.shared.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
let hiddenPrivateOutside: Wu6PrivateFragmentControl.FirstOnly; // error[TK2694]: Namespace 'Wu6PrivateFragmentControl' has no exported member 'FirstOnly'
