// Backlog 103 correctness: a legal interface augmentation preserves an inherited class identity.
interface SafeArray<T = any> {
  b103Value(): T;
}

declare const safe: SafeArray<number>;
const value: number = safe.b103Value();
const wrong: string = safe.b103Value(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const array: number[] = new VBArray(safe).toArray();
