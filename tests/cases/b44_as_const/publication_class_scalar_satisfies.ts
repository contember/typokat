class ScalarSatisfiesFields {
  text = "a" satisfies "a" | number;
  numeric = 1 satisfies 1 | string;
  negative = -1 satisfies -1 | string;
  readonly retained = "a" satisfies "a" | number;
  exact = ("a" as const) satisfies "a" | number;
  annotated: "a" = "a" satisfies "a" | number;
  nested = { value: "a" } satisfies { value: "a" | number };
  array = ["a"] satisfies ("a" | number)[];
  parenthesized = (("a" satisfies "a" | number));
}

const scalarFields = new ScalarSatisfiesFields();
const exactText: "a" = scalarFields.text; // error[TK2322]
const exactNumber: 1 = scalarFields.numeric; // error[TK2322]
const exactNegative: -1 = scalarFields.negative; // error[TK2322]
const exactParenthesized: "a" = scalarFields.parenthesized; // error[TK2322]
let textCopy = scalarFields.text;
textCopy = "b";
let numericCopy = scalarFields.numeric;
numericCopy = 2;
let negativeCopy = scalarFields.negative;
negativeCopy = -2;

const readonlyExact: "a" = scalarFields.retained;
let readonlyCopy = scalarFields.retained;
readonlyCopy = "b";
let exactCopy = scalarFields.exact;
exactCopy = "b"; // error[TK2322]
let annotatedCopy = scalarFields.annotated;
annotatedCopy = "b"; // error[TK2322]
let nestedCopy = scalarFields.nested.value;
nestedCopy = "b"; // error[TK2322]
let arrayCopy = scalarFields.array[0];
arrayCopy = "b"; // error[TK2322]
