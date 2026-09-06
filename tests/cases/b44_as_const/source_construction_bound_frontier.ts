declare function graph<A extends Z, Other extends { p: E }, Z extends D | E, D extends { e: E }, E extends { d: D }>(): D;
const result = graph();
const legal: { e: { d: unknown } } = result;
const illegal: { e: { d: { e: unknown } } } = result; // error[TK2322]
