// One non-widening contribution cannot freeze a different ordinary literal contribution.

declare const mixedCondition: boolean;
const mixedLeft = mixedCondition ? 0 : 1;
let viaIdentifier = mixedLeft || (2 as const);
viaIdentifier = 3;
let inlineLogical = (mixedCondition ? 0 : 1) || (2 as const);
inlineLogical = 3;

let exactLogical = mixedCondition && (1 as const);
exactLogical = 2; // error[TK2322]

let mixedUnion = mixedCondition ? ("exact" as const) : 1;
mixedUnion = 2;
mixedUnion = "other"; // error[TK2322]
let reversedUnion = mixedCondition ? 1 : ("exact" as const);
reversedUnion = 2;
reversedUnion = "other"; // error[TK2322]

let equalValue = mixedCondition ? (1 as const) : 1;
equalValue = 2; // error[TK2322]
let reversedEqualValue = mixedCondition ? 1 : (1 as const);
reversedEqualValue = 2; // error[TK2322]
