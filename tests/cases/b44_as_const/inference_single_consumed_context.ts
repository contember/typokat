declare function consumeScalarContext<T>(value: T, callback: (value: T) => void): T;
declare function consumeNestedScalarContext<T>(value: T, callback: { run: (value: T) => void }): T;

const consumedScalar = consumeScalarContext("read", value => {
  const exactBody: "read" = value; // error[TK2322]
});
const exactConsumedScalar: "read" = consumedScalar; // error[TK2322]
let consumedScalarCopy = consumedScalar;
consumedScalarCopy = "write";

const consumedNestedScalar = consumeNestedScalarContext("read", { run: value => {
  const exactNestedBody: "read" = value; // error[TK2322]
} });
const exactConsumedNested: "read" = consumedNestedScalar; // error[TK2322]
let consumedNestedCopy = consumedNestedScalar;
consumedNestedCopy = "write";

const consumedRegular = consumeScalarContext("read" as const, value => {
  const exactRegularBody: "read" = value;
});
const exactConsumedRegular: "read" = consumedRegular;
let consumedRegularCopy = consumedRegular;
consumedRegularCopy = "write"; // error[TK2322]

const consumedRegularNested = consumeNestedScalarContext("read" as const, { run: value => {
  const exactRegularNestedBody: "read" = value;
} });
let consumedRegularNestedCopy = consumedRegularNested;
consumedRegularNestedCopy = "write"; // error[TK2322]
