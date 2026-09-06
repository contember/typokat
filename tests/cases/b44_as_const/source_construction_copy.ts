const alias = graph;
declare function graph<A extends C | B, B extends { c: C }, C extends { b: B }>(): B;
const result = alias();
const legal: { c: unknown } = result;
const illegal: { c: { b: unknown } } = result; // error[TK2322]
