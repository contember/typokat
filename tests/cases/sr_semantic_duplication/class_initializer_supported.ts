// Semantic-duplication architecture gate — representative supported initializer boundaries.
// tsc 6.0.3 --strict is clean except for the deliberate assignment controls below.
// Each field is unannotated: class surface construction must infer it without poisoning the class.

class SupportedInitializerMatrix {
  mutableLiteral = 1;
  readonly readonlyLiteral = 1;
  objectLiteral = { value: 1 };
  arrayLiteral = [1, 2];
  parenthesized = (1);
  unary = -1;
  assertion = 1 as number;
  backwardThis = this.mutableLiteral;

  readNumber(): number {
    return 1;
  }

  thisCall = this.readNumber();
  expressionArrow = (value: number) => value;
}

declare const supportedInitializers: SupportedInitializerMatrix;

const mutableLiteralGood: number = supportedInitializers.mutableLiteral;
const mutableLiteralBad: string = supportedInitializers.mutableLiteral; // error[TK2322]: Type 'number' is not assignable to type 'string'
const readonlyLiteralGood: 1 = supportedInitializers.readonlyLiteral;
const readonlyLiteralBad: 2 = supportedInitializers.readonlyLiteral; // error[TK2322]: Type '1' is not assignable to type '2'
const objectLiteralGood: number = supportedInitializers.objectLiteral.value;
const objectLiteralBad: string = supportedInitializers.objectLiteral.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const arrayLiteralGood: number = supportedInitializers.arrayLiteral[0];
const arrayLiteralBad: string = supportedInitializers.arrayLiteral[0]; // error[TK2322]: Type 'number' is not assignable to type 'string'
const parenthesizedGood: number = supportedInitializers.parenthesized;
const unaryGood: number = supportedInitializers.unary;
const assertionGood: number = supportedInitializers.assertion;
const backwardThisGood: number = supportedInitializers.backwardThis;
const thisCallGood: number = supportedInitializers.thisCall;
const arrowGood: number = supportedInitializers.expressionArrow(1);
const arrowBad: string = supportedInitializers.expressionArrow(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
