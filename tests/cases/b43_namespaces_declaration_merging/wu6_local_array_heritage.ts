// tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs: TS2322.

interface Array<T> {
  wu6LocalElement: T;
}

interface Wu6StringArray extends Array<string> {
  own: boolean;
}

declare const wu6StringArray: Wu6StringArray;
const inheritedLocalElement: string = wu6StringArray.wu6LocalElement;
const inheritedLocalElementWrong: number = wu6StringArray.wu6LocalElement; // error[TK2322]: Type 'string' is not assignable to type 'number'
const ownMemberStillPresent: boolean = wu6StringArray.own;
