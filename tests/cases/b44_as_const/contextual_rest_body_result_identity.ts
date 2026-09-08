type ResultIdentityRows = <T extends ["n", { value: number }] | ["s", { value: string }]>(...args: T) => T[1];
const resultIdentityRows = ((kind, payload) => {
  payload.value;
  return payload;
}) satisfies ResultIdentityRows;

const resultIdentityNumber = resultIdentityRows<["n", { value: 1 }]>("n", { value: 1 });
const resultIdentityNarrowNumber: 1 = resultIdentityNumber.value; // error[TK2322]
const resultIdentityWrongNumber: 2 = resultIdentityNumber.value; // error[TK2322]
const resultIdentityString = resultIdentityRows<["s", { value: "kept" }]>("s", { value: "kept" });
const resultIdentityNarrowString: "kept" = resultIdentityString.value; // error[TK2322]
const resultIdentityWrongString: "other" = resultIdentityString.value; // error[TK2322]
resultIdentityRows<["n", { value: 1 }]>("n", { value: 2 }); // error[TK2322]

type RepeatedTagRows = (...args: ["a", number] | ["a", string] | ["b", { value: boolean }]) => void;
const repeatedTagRows: RepeatedTagRows = (kind, payload) => {
  if (kind === "a") {
    payload.toFixed(); // error[TK2339]
    if (typeof payload === "number") payload.toFixed();
    if (typeof payload === "string") payload.toUpperCase();
  }
  if (typeof payload === "number") {
    const independentTag: "a" = kind; // error[TK2322]
    if (kind === "a") payload.toFixed();
  }
};
