// A selected contextual signature publishes the complete literal type of its parameter.

declare function consumeLiteral<T>(value: T, callback: (value: T) => void): void;
consumeLiteral(1 as const, (value) => {
  let copy = value;
  copy = 2; // error[TK2322]
});

const exactCallback: (value: 1) => void = (value) => {
  let copy = value;
  copy = 2; // error[TK2322]
};

const broadCallback: (value: number) => void = (value) => {
  let copy = value;
  copy = 2;
};
