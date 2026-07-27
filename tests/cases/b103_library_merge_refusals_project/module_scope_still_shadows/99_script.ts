// The sibling script file: the module's shadows are invisible here, so `Array`/`Date` keep the
// library surface. See 00_module.ts for the oracle.
const scriptElement = [1, 2].b103CrossElement; // error[TK2339]
const scriptStamp = new Date().b103CrossStamp(); // error[TK2339]
