// tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs: TS2322.

interface ReadonlyArray<T> {
  wu6ReadonlyLocalElement: T;
}

interface Wu6ReadonlyStringArray extends ReadonlyArray<string> {
  own: boolean;
}

declare const wu6ReadonlyStringArray: Wu6ReadonlyStringArray;
const inheritedReadonlyLocalElement: string = wu6ReadonlyStringArray.wu6ReadonlyLocalElement;
const inheritedReadonlyLocalElementWrong: number = wu6ReadonlyStringArray.wu6ReadonlyLocalElement; // error[TK2322]: Type 'string' is not assignable to type 'number'
const readonlyOwnMemberStillPresent: boolean = wu6ReadonlyStringArray.own;
