declare function inferArrayFromResult<T>(): T[];

const inferredReadonlyStrings: ReadonlyArray<string> = inferArrayFromResult();
const explicitReadonlyStrings: ReadonlyArray<string> = inferArrayFromResult<string>();
const wrongReadonlyStrings: ReadonlyArray<string> = inferArrayFromResult<number>(); // error[TK2322]
