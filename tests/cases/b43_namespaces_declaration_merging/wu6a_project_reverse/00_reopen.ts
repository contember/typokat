// WU6A reverse project reopening; tsc 6.0.3 uses the same strict explicit-input command.

namespace Wu6aProjectOrder {
  export const second: string = "two";
  export namespace Nested {
    export let count: number = 1;
  }
}

Wu6aProjectOrder.Nested.count = 2;
