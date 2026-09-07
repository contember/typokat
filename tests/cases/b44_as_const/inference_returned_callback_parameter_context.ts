declare function returnedCallback<T>(seed: T, body: () => (value: T) => void): T;
declare function returnedObjectCallback<T>(seed: T, body: () => { run: (value: T) => void }): T;
declare function returnedTupleCallback<T>(seed: T, body: () => [(value: T) => void]): T;

const returnedCallbackWrite: number = returnedCallback(1 as const, () => value => {
  value = 2; // error[TK2322]
});
const returnedCallbackRead: number = returnedCallback(1 as const, () => value => {
  const exactReturnedParameter: 1 = value;
});

const returnedObjectWrite: number = returnedObjectCallback(1 as const, () => ({ run: value => {
  value = 2; // error[TK2322]
} }));
const returnedObjectRead: number = returnedObjectCallback(1 as const, () => ({ run: value => {
  const exactReturnedObjectParameter: 1 = value;
} }));

const returnedTupleWrite: number = returnedTupleCallback(1 as const, () => [value => {
  value = 2; // error[TK2322]
}]);
const returnedTupleRead: number = returnedTupleCallback(1 as const, () => [value => {
  const exactReturnedTupleParameter: 1 = value;
}]);
