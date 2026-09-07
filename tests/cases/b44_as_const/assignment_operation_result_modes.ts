let operationNumber = 0;
let operationAddition = (operationNumber += 1 as const);
operationAddition = 2;
operationAddition = "wrong"; // error[TK2322]

let operationString = "";
let operationConcatenation = (operationString += 1 as const);
operationConcatenation = "other";
operationConcatenation = 2; // error[TK2322]
let operationStringLiteral = (operationString += "suffix" as const);
operationStringLiteral = "different";
operationStringLiteral = 3; // error[TK2322]

let operationSubtraction = (operationNumber -= 1 as const);
operationSubtraction = 3;
operationSubtraction = "wrong"; // error[TK2322]
let operationMultiplication = (operationNumber *= 2 as const);
operationMultiplication = 4;
operationMultiplication = false; // error[TK2322]

declare let operationNullable: 1 | undefined;
let operationNullish = (operationNullable ??= 1 as const);
operationNullish = 1;
operationNullish = 2; // error[TK2322]

let operationConjunction = true;
let operationAnd = (operationConjunction &&= false as const);
operationAnd = false;
operationAnd = true; // error[TK2322]

declare let operationDisjunction: 0 | 1;
let operationOr = (operationDisjunction ||= 1 as const);
operationOr = 1;
operationOr = 0; // error[TK2322]
