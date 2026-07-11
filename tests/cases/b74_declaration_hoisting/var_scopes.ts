// Backlog 74 — var belongs to the containing function/module, while let/const
// remain lexical. Cross-checked against tsc 6.0.3 --strict.

{
  var moduleVar: number = 1;
}
const moduleVarOk: number = moduleVar;
const moduleVarWrong: string = moduleVar; // error[TK2322]: Type 'number' is not assignable to type 'string'

function blockVarScope(): void {
  forwardBlockVar = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
  {
    var blockVar: number = 1;
    var forwardBlockVar: number;
  }
  const blockVarOk: number = blockVar;
  const blockVarWrong: string = blockVar; // error[TK2322]: Type 'number' is not assignable to type 'string'
}

function ifVarScope(): void {
  if (true) {
    var ifVar: number;
  }
  ifVar = 1;
  ifVar = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}

function switchVarScope(value: number): void {
  switch (value) {
    case 0:
      var switchVar: number;
      break;
  }
  switchVar = 1;
  switchVar = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}

function lexicalControls(): void {
  {
    let blockLet = 1;
    const blockConst = 2;
  }
  blockLet = 3; // error[TK2304]: Cannot find name 'blockLet'
  const missingConst: number = blockConst; // error[TK2304]: Cannot find name 'blockConst'
}

function catchParameterCollision(): void {
  try {
  } catch (caught) { // incomplete[stmt-check/try-statement/catch-param]
    var caught: number = 1;
  }
  caught = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}

function lexicalFunctionCollision(): void {
  {
    function value(): number {
      return 1;
    }
    var value = 1;
  }
  value = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}

function inferredVarRefreshesFlowMemo(flag: boolean): void {
  if (flag) {
    const before: unknown = later;
    var later = 1;
    const after: string = later; // error[TK2322]: Type 'number' is not assignable to type 'string'
  }
}
