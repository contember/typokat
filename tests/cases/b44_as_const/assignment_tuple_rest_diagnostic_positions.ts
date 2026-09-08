declare let restPositionTarget: [number, ...string[], boolean];
restPositionTarget = [1, true];
restPositionTarget = [1, "ok", true];
restPositionTarget = [1, "one", "two", true];
restPositionTarget = [1, 2, true]; // error[TK2322]
restPositionTarget = [1, "ok", "wrong"]; // error[TK2322]

declare let leadingRestPositionTarget: [...string[], boolean];
leadingRestPositionTarget = [true];
leadingRestPositionTarget = ["ok", true];
leadingRestPositionTarget = ["one", "two", true];
leadingRestPositionTarget = [2, true]; // error[TK2322]
leadingRestPositionTarget = ["ok", "wrong"]; // error[TK2322]

declare let nestedRestPositionTarget: [number, ...{ value: number }[], { last: boolean }];
nestedRestPositionTarget = [1, { last: true }];
nestedRestPositionTarget = [1, { value: 2 }, { value: 3 }, { last: true }];
nestedRestPositionTarget = [1, { value: "wrong" }, { last: true }]; // error[TK2322]
nestedRestPositionTarget = [1, { value: 2 }, { last: "wrong" }]; // error[TK2322]

declare let nestedLeadingRestPositionTarget: [...{ value: number }[], { last: boolean }];
nestedLeadingRestPositionTarget = [{ last: true }];
nestedLeadingRestPositionTarget = [{ value: 2 }, { last: true }];
nestedLeadingRestPositionTarget = [{ value: "wrong" }, { last: true }]; // error[TK2322]
nestedLeadingRestPositionTarget = [{ value: 2 }, { last: "wrong" }]; // error[TK2322]
