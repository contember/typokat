// Backlog 103 correctness: illegal aliases stay library-winning without a frozen-write refusal.
type Partial<T> = { b103: T };

const partial: Partial<number> = { b103: 1 }; // error[TK2322]
