// F1 / backlog 05 (WU2) - callability dispatch rejects values without a call
// signature, including non-function properties.
// Cross-checked against tsc 6.0.3 --strict.

type ObjectOnly = {
  value: number;
};

declare const objectOnly: ObjectOnly;
objectOnly(1);      // error[TK2349]: This expression is not callable

type Item = {
  value: number;
};

declare const item: Item;
item.value();       // error[TK2349]: This expression is not callable
