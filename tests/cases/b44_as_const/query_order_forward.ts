// The same readonly-to-mutable relation must fail before or after a successful readonly
// relation query. This file asks the failing query first.

const forwardFrozen = (["forward", { value: 1 }]) as const;

declare function forwardMutable(value: ["forward", { value: number }]): void;
declare function forwardReadonly(value: readonly ["forward", { readonly value: 1 }]): void;

forwardMutable(forwardFrozen); // error[TK2345]
forwardReadonly(forwardFrozen);

const forwardExact: readonly ["forward", { readonly value: 1 }] = forwardFrozen;
const forwardValue: 1 = forwardFrozen[1].value;
