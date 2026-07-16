// WU6A forward project reopening; see 00_consume.ts for the pinned command.

namespace Wu6aProjectOrder {
  export const second: string = "two";
  export namespace Nested {
    export let count: number = 1;
  }
}

Wu6aProjectOrder.Nested.count = 2;
