// Reverse of query_order_forward.ts: the successful readonly query runs before the same
// readonly-to-mutable rejection, so cache/query order cannot drop the failure.

const reverseFrozen = (["reverse", { value: 1 }]) as const;

declare function reverseMutable(value: ["reverse", { value: number }]): void;
declare function reverseReadonly(value: readonly ["reverse", { readonly value: 1 }]): void;

reverseReadonly(reverseFrozen);
reverseMutable(reverseFrozen); // error[TK2345]

const reverseExact: readonly ["reverse", { readonly value: 1 }] = reverseFrozen;
const reverseValue: 1 = reverseFrozen[1].value;
