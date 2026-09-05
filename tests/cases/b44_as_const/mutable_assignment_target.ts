// A mutable declaration governs the target surface even when its assigned source is readonly.

const readonlySource = ({ value: 1 }) as const;
let mutableTarget: { value: number } = { value: 0 };
mutableTarget = readonlySource;
let mutableCopy = mutableTarget;
mutableCopy.value = 2;

let inlineTarget: { value: number } = { value: 0 };
inlineTarget = ({ value: 1 }) as const;
let inlineCopy = inlineTarget;
inlineCopy.value = 2;

const readonlyReference = readonlySource;
readonlyReference.value = 1; // error[TK2540]
