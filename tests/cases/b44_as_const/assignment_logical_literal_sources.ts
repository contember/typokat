declare const logicalLiteralLeft: { value: number } | undefined;
declare let logicalLiteralTarget: { value: number };
logicalLiteralTarget = logicalLiteralLeft ?? { value: 1 };
logicalLiteralTarget = logicalLiteralLeft || { value: 1 };
logicalLiteralTarget = (logicalLiteralLeft ?? { value: 1 });
logicalLiteralTarget.value = "wrong"; // error[TK2322]

declare let logicalLiteralOptional: { value: number } | undefined;
logicalLiteralOptional = logicalLiteralLeft && { value: 1 };

declare const logicalLiteralChoice: boolean;
logicalLiteralTarget = logicalLiteralChoice ? { value: 1 } : { value: 2 };

declare const logicalLiteralFalse: false;
declare let logicalLiteralFalseTarget: false;
declare function logicalLiteralNumber(value: number): number;
logicalLiteralFalseTarget = logicalLiteralFalse && logicalLiteralNumber("wrong"); // error[TK2345]
