// Backlog 48 WU0 — flat object variable binding leaves get independent value types.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit`.

const b48ConstSource: { count: number; title: string; active?: boolean } = {
  count: 1,
  title: "ready",
};
const { count, title: localTitle, active = false } = b48ConstSource;
const b48ConstCount: number = count;
const b48ConstCountWrong: string = count; // error[TK2322]
const b48ConstTitle: string = localTitle;
const b48ConstTitleWrong: number = localTitle; // error[TK2322]
const b48ConstActive: boolean = active;
const b48ConstActiveWrong: string = active; // error[TK2322]

let { count: mutableCount, title: mutableTitle } = b48ConstSource;
mutableCount = 2;
mutableCount = "bad"; // error[TK2322]
mutableTitle = "changed";
mutableTitle = 3; // error[TK2322]

var { count: varCount, title: varTitle } = b48ConstSource;
const b48VarCount: number = varCount;
const b48VarCountWrong: string = varCount; // error[TK2322]
const b48VarTitle: string = varTitle;
const b48VarTitleWrong: number = varTitle; // error[TK2322]

const b48StaticKeySource: {
  plain: number;
  "string-key": string;
  7: boolean;
} = { plain: 1, "string-key": "seven", 7: true };
const {
  plain: identifierKeyLeaf,
  "string-key": stringKeyLeaf,
  7: numberKeyLeaf,
} = b48StaticKeySource;
const b48IdentifierKeyWrong: string = identifierKeyLeaf; // error[TK2322]
const b48StringKeyWrong: number = stringKeyLeaf; // error[TK2322]
const b48NumberKeyWrong: string = numberKeyLeaf; // error[TK2322]
