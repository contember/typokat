// WU0b GREEN parity: supported namespace/global owners keep their current order.

export {};

namespace InventorySpace {
  export interface NestedInterface {
    value: string;
  }
}

const invalidNamespace: InventorySpace.NestedInterface = { value: 1 }; // error[TK2322]

declare global {
  interface InventoryGlobal {
    value: string;
  }
}

const invalidGlobal: InventoryGlobal = { value: 1 }; // error[TK2322]
