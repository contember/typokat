declare function graph<B extends { c: C }, C extends { b: B }, A extends B | C>(): B;
const result = graph();
const legal: { c: { b: unknown } } = result;
const illegal: { c: { b: { c: unknown } } } = result; // error[TK2322]
