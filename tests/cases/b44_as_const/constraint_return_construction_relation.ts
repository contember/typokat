// A forward callable relation can demand declared return constituents before constraints.

const earlyReturnConstructionRelation: () => [unknown, unknown, unknown, unknown, unknown] = returnConstructionRelation;
declare function returnConstructionRelation<A extends C[] | B[], B extends { c: C }, C extends { b: B }>(): [B[], C[], A, B, C];
const returnConstructionRelationResult = returnConstructionRelation();
const legalReturnConstructionRelation: { b: unknown } = returnConstructionRelationResult[3].c;
const illegalReturnConstructionRelation: { c: unknown } = returnConstructionRelationResult[4].b; // error[TK2322]
