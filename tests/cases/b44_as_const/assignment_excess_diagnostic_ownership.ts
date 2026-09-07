declare const excessOwnershipSource: { value: number };
let excessOwnershipTarget: { value: number } | { tag: string } = { tag: "other" };
excessOwnershipTarget = excessOwnershipSource;
let excessOwnershipCopy = excessOwnershipTarget;
excessOwnershipCopy = { tag: "other" }; // error[TK2353]

let excessOwnershipDirect: { value: number } = { value: 0 };
excessOwnershipDirect = { tag: "other" }; // error[TK2353]
excessOwnershipDirect = {}; // error[TK2741]
excessOwnershipDirect = { value: 1, extra: true }; // error[TK2353]
excessOwnershipDirect = { value: "wrong", extra: true }; // error[TK2322]

declare function excessOwnershipNumber(value: number): number;
excessOwnershipDirect = { tag: excessOwnershipNumber("wrong") }; // error[TK2353] | error[TK2345]
