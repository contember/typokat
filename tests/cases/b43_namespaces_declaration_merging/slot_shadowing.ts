// tsc 6.0.3 --strict: only the locally shadowed namespace member reports TS2694.
namespace SlotRoot {
  export interface Item { item: true }
}

function valueSlotDoesNotBlockNamespace() {
  const SlotRoot = 1;
  let item: SlotRoot.Item;
}

function typeSlotDoesNotBlockNamespace() {
  interface SlotRoot { local: true }
  let item: SlotRoot.Item;
}

namespace NamespaceHost {
  export namespace SlotRoot {
    export interface Local { local: true }
  }
  export namespace Nested {
    namespace SlotRoot {
      export interface Inner { inner: true }
    }
    let blockedParentNamespace: SlotRoot.Item; // error[TK2694]: Namespace 'NamespaceHost.Nested.SlotRoot' has no exported member 'Item'
    let localNamespaceWins: SlotRoot.Inner;
  }
}

const valueSlotControl = valueSlotDoesNotBlockNamespace;
const typeSlotControl = typeSlotDoesNotBlockNamespace;
