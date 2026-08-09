// Controls: ordinary declarations, F4 access checks, and a B103 library-value collision keep
// their established behavior while flat object leaves acquire types.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit`.

const b48OrdinaryConst = 1;
let b48OrdinaryLet: string = "ready";
var b48OrdinaryVar: boolean = true;
const b48OrdinaryWrong: string = b48OrdinaryConst; // error[TK2322]
b48OrdinaryLet = 1; // error[TK2322]
b48OrdinaryVar = "bad"; // error[TK2322]

class B48AccessHolder {
  private hidden: number = 1;
  public visible: string = "ready";
}
const { hidden: hiddenLeaf, visible: visibleLeaf } = new B48AccessHolder(); // error[TK2341]
const b48VisibleLeaf: string = visibleLeaf;
const b48VisibleLeafWrong: number = visibleLeaf; // error[TK2322]

declare const b48RegExpSource: { ctor: RegExpConstructor };
var { ctor: RegExp } = b48RegExpSource;
const b48ConstructedRegExp: RegExp = new RegExp("x");
const b48RegExpTest: boolean = /x/.test("x");
const b48RegExpTestWrong: string = /x/.test("x"); // error[TK2322]
