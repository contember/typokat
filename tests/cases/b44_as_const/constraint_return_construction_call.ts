// A forward call demands constraint constituents before the declared return constituents.

const earlyReturnConstructionCall = returnConstructionCall();
declare function returnConstructionCall<A extends C[] | B[], B extends { c: C }, C extends { b: B }>(): [B[], C[], A, B, C];
const returnConstructionCallResult = returnConstructionCall();
const illegalReturnConstructionCall: { b: unknown } = returnConstructionCallResult[3].c; // error[TK2322]
const legalReturnConstructionCall: { c: unknown } = returnConstructionCallResult[4].b;
