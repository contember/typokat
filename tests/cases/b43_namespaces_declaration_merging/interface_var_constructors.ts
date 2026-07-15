// tsc 6.0.3 --strict: TS2741 x2 and TS2322 x2; constructors return the final merged interface.
interface ForwardConstructor {
  first: number;
}
interface ForwardConstructor {
  second: string;
}
declare var ForwardConstructor: {
  new (): ForwardConstructor;
};
const forwardConstructed: ForwardConstructor = new ForwardConstructor();
const forwardConstructedWrong: number = forwardConstructed.second; // error[TK2322]: Type 'string' is not assignable to type 'number'
const forwardIncomplete: ForwardConstructor = { first: 1 }; // error[TK2741]

declare var ReverseConstructor: {
  new (): ReverseConstructor;
};
interface ReverseConstructor {
  first: number;
}
interface ReverseConstructor {
  second: string;
}
const reverseConstructed: ReverseConstructor = new ReverseConstructor();
const reverseConstructedWrong: string = reverseConstructed.first; // error[TK2322]: Type 'number' is not assignable to type 'string'
const reverseIncomplete: ReverseConstructor = { second: "ok" }; // error[TK2741]

interface DateLike {
  valueOf(): number;
}
interface DateLike {
  toText(): string;
}
declare var DateLike: {
  new (): DateLike;
  readonly prototype: DateLike;
};
const dateLike: DateLike = new DateLike();
const datePrototype: DateLike = DateLike.prototype;
