interface String {
  b103SplitUpper(): string;
}

declare const view: Window;
const splitFlag: boolean = view.b103SplitFlag;
const wrongSplitFlag: string = view.b103SplitFlag; // error[TK2322]
const splitUpper: string = "quiet".b103SplitUpper();
const wrongSplitUpper: number = "quiet".b103SplitUpper(); // error[TK2322]
const nativeWidth: number = view.innerWidth;
const nativeLength: number = "quiet".length;
