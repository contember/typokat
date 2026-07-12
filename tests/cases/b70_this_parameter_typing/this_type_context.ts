// Backlog 70 — ThisType<T> supplies the contextual receiver inside object-literal
// methods while remaining structurally empty for assignability.

type NumericContext = {
  n: number;
  check(): void;
} & ThisType<{ n: number }>;

const numeric: NumericContext = {
  n: 1,
  check() {
    const numericValue: number = this.n;
  },
};

type StringContext = {
  n: number;
  check(): void;
} & ThisType<{ n: string }>;

const stringContext: StringContext = {
  n: 1,
  check() {
    const wrongValue: number = this.n; // error[TK2322]
  },
};
