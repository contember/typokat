const optionalExpressionReturn: () => "a" | undefined = () => "a";
const optionalBlockReturn: () => "a" | undefined = () => { return "a"; };
const nullableExpressionReturn: () => "a" | null = () => "a";
const nullableBlockReturn: () => "a" | null = () => { return "a"; };
const mixedExpressionReturn: () => "a" | number = () => "a";
const mixedBlockReturn: () => "a" | number = () => { return "a"; };
const numericExpressionReturn: () => 1 | string = () => 1;
const numericBlockReturn: () => 1 | string = () => { return 1; };
const booleanExpressionReturn: () => boolean = () => true;
const booleanBlockReturn: () => boolean = () => { return true; };

const exactOptionalExpression = (() => "a") satisfies () => "a" | undefined;
const exactOptionalExpressionValue: "a" = exactOptionalExpression();
const exactOptionalBlock = (() => { return "a"; }) satisfies () => "a" | undefined;
const exactOptionalBlockValue: "a" = exactOptionalBlock();
const exactBooleanExpression = (() => true) satisfies () => boolean;
const exactBooleanExpressionValue: true = exactBooleanExpression();
const exactBooleanBlock = (() => { return true; }) satisfies () => boolean;
const exactBooleanBlockValue: true = exactBooleanBlock();
const exactMixedExpression = (() => 1) satisfies () => 1 | string;
const exactMixedExpressionValue: 1 = exactMixedExpression();
const exactMixedBlock = (() => { return 1; }) satisfies () => 1 | string;
const exactMixedBlockValue: 1 = exactMixedBlock();

const broadExpression = (() => "a") satisfies () => string;
const broadExpressionValue: "a" = broadExpression(); // error[TK2322]
const broadBlock = (() => { return "a"; }) satisfies () => string;
const broadBlockValue: "a" = broadBlock(); // error[TK2322]
const broadNumericExpression = (() => 1) satisfies () => number;
const broadNumericExpressionValue: 1 = broadNumericExpression(); // error[TK2322]
const broadNumericBlock = (() => { return 1; }) satisfies () => number;
const broadNumericBlockValue: 1 = broadNumericBlock(); // error[TK2322]

const invalidExpressionReturn: () => "a" | undefined = () => "b"; // error[TK2322]
const invalidBlockReturn: () => "a" | undefined = () => { return "b"; }; // error[TK2322]
