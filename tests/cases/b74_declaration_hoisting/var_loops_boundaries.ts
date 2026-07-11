// Backlog 74 — loop var bindings hoist only to the containing function, and
// moving the binding must not move initializer lookup. Cross-checked against
// tsc 6.0.3 --strict.

function loopVarScopes(source: number[]): void {
  for (var forVar: number = 0; false;) {}
  forVar = 1;

  for (var forInVar in { key: 1 }) {}
  forInVar = "key";

  for (var forOfVar of source) {}
  forOfVar = 1;

  while (false) {
    var whileVar: number;
  }
  whileVar = 1;

  do {
    var doVar: number = 1;
  } while (false);
  const doVarOk: number = doVar;
}

function initializerKeepsLexicalScope(): void {
  const source: string = "outer";
  {
    const source: number = 1;
    var initializedInBlock: number = source;
  }
  const initializedOk: number = initializedInBlock;
}

function outerBoundary(): void {
  {
    var outerCaptured: number = 1;
  }
  function innerBoundary(): void {
    const captureOk: number = outerCaptured;
    var innerOnly: number = 1;
  }
  innerOnly = 2; // error[TK2304]: Cannot find name 'innerOnly'
}
