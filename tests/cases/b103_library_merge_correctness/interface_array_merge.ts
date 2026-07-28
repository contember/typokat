// Backlog 103 correctness: a legal Array augmentation merges without hiding the native surface.
interface Array<T> {
  b103First(): T;
}

const first: number = [1, 2, 3].b103First();
const wrongFirst: string = [1, 2, 3].b103First(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const mapped: number[] = [1, 2, 3].map((value) => value + 1);
const wrongMapped: string[] = [1, 2, 3].map((value) => value + 1); // error[TK2322]
