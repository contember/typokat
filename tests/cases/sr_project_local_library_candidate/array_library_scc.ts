declare const concatenable: ConcatArray<number>;

const combined = [1, 2, 3].concat(concatenable);
const sliced = concatenable.slice();
const combinedValue: number = combined[0];
const slicedValue: number = sliced[0];
const wrongCombinedValue: string = combined[0]; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongSlicedValue: string = sliced[0]; // error[TK2322]: Type 'number' is not assignable to type 'string'
const reflectedKeys: (string | symbol)[] = Reflect.ownKeys({ value: 1 });
const wrongReflectedKeys: number = Reflect.ownKeys({ value: 1 }); // error[TK2322]: Type '(string | symbol)[]' is not assignable to type 'number'
