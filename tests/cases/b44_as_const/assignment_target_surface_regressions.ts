// An exact source neither replaces a broad declared target nor survives an invalid write.

function preserveTemplateTarget(exact: "aA", template: `aA${string}`) {
  template = exact;
  exact = template; // error[TK2322]
}

function preservePrimitiveTarget(exact: "a", broad: string) {
  broad = exact;
  exact = broad; // error[TK2322]
}

function invalidDisjointTarget(source: 1, target: 2 | 3) {
  target = source; // error[TK2322]
  target = target;
}

function invalidPartialTarget(source: 1 | 2 | 3, target: 0 | 1 | 2) {
  target = source; // error[TK2322]
  target = target;
}

let compoundModeTarget = 0;
let compoundModeResult = (compoundModeTarget += 1 as const);
compoundModeResult = 2;
