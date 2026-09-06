const result = graph();
declare function graph<X extends B[], A extends C[] | B[], B extends { c: C }, C extends { b: B }>(): B;
const legal: { c: { b: unknown } } = result;
const illegal: { c: { b: { c: unknown } } } = result; // error[TK2322]
