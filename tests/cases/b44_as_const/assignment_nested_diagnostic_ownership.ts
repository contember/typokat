declare let nestedOwnershipTarget: { nested: { value: number } };
nestedOwnershipTarget = { nested: { extra: true } }; // error[TK2353]
nestedOwnershipTarget = { nested: {} }; // error[TK2741]
nestedOwnershipTarget = { nested: {}, extra: true }; // error[TK2741]

declare let optionalOwnershipTarget: { nested?: { value: number } };
optionalOwnershipTarget = { nested: { extra: true } }; // error[TK2353]

declare let indexedOwnershipTarget: { [key: string]: { value: number } };
indexedOwnershipTarget = { item: { extra: true } }; // error[TK2353]

declare let siblingOwnershipTarget: { left: { value: number }; right: { value: number } };
siblingOwnershipTarget = { left: { value: "wrong" }, right: { extra: true } }; // error[TK2322] | error[TK2353]
siblingOwnershipTarget = { right: { extra: true }, left: { value: "wrong" } }; // error[TK2353] | error[TK2322]
siblingOwnershipTarget = { left: { extra: true }, right: { extra: true } }; // error[TK2353] | error[TK2353]

declare let arrayOwnershipTarget: { value: number }[];
arrayOwnershipTarget = [{ extra: true }, { value: "wrong" }]; // error[TK2353] | error[TK2322]

declare function nestedOwnershipNumber(value: number): number;
siblingOwnershipTarget = { left: { extra: nestedOwnershipNumber("wrong") }, right: { extra: true } }; // error[TK2353] | error[TK2345] | error[TK2353]
