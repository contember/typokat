const result = graph();
declare function graph<A extends C[] | B[] | C[], B extends { c: C }, C extends { b: B }>(): B;
const legal: { c: unknown } = result;
const illegal: { c: { b: unknown } } = result; // error[TK2322]
