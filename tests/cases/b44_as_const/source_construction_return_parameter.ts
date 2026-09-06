const result = graph();
declare function graph<A extends C | B, B extends { c: C }, C extends { b: B }>(): C;
const legal: { b: unknown } = result;
const illegal: { b: { c: unknown } } = result; // error[TK2322]
