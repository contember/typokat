let booleanUnionFalse: boolean | 1 = 1;
booleanUnionFalse = false as const;
const booleanUnionFalseRead: false = booleanUnionFalse;
let booleanUnionFalseCopy = booleanUnionFalse;
booleanUnionFalseCopy = true; // error[TK2322]

let booleanUnionTrue: 1 | boolean = 1;
booleanUnionTrue = true as const;
const booleanUnionTrueRead: true = booleanUnionTrue;
let booleanUnionTrueCopy = booleanUnionTrue;
booleanUnionTrueCopy = false; // error[TK2322]

declare const booleanUnionExactSource: false;
let booleanUnionIdentifier: boolean | 1 = 1;
booleanUnionIdentifier = booleanUnionExactSource;
let booleanUnionIdentifierCopy = booleanUnionIdentifier;
booleanUnionIdentifierCopy = true; // error[TK2322]

declare const booleanUnionMixedSource: false | 1;
let booleanUnionMixedTarget: boolean | 1 | 2 = 2;
booleanUnionMixedTarget = booleanUnionMixedSource;
let booleanUnionMixedCopy = booleanUnionMixedTarget;
booleanUnionMixedCopy = true; // error[TK2322]
booleanUnionMixedCopy = 2; // error[TK2322]

declare const booleanUnionBroadSource: boolean;
let booleanUnionBroadTarget: 1 | boolean = 1;
booleanUnionBroadTarget = booleanUnionBroadSource;
let booleanUnionBroadCopy = booleanUnionBroadTarget;
booleanUnionBroadCopy = false;
booleanUnionBroadCopy = true;
booleanUnionBroadCopy = 1; // error[TK2322]

let booleanUnionFreshTarget: boolean | 1 = 1;
booleanUnionFreshTarget = false;
const booleanUnionFreshRead: false = booleanUnionFreshTarget;
let booleanUnionFreshCopy = booleanUnionFreshTarget;
booleanUnionFreshCopy = true;
booleanUnionFreshCopy = 1; // error[TK2322]
