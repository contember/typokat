// Assignment values use the RHS's actual contextual type, not its raw pre-context type.

let contextualTupleTarget: [number, number] = [0, 0];
let contextualTupleResult = (contextualTupleTarget = [1, 2]);
contextualTupleResult = [1, 2, 3]; // error[TK2322]

let contextualObjectTarget: { kind: "a"; value: number } | { kind: "b"; value: string } = { kind: "b", value: "" };
let contextualObjectResult = (contextualObjectTarget = { kind: "a", value: 1 });
contextualObjectResult.kind = "b"; // error[TK2322]
contextualObjectResult.value = 2;
contextualObjectResult.value = "wrong"; // error[TK2322]

let contextualFunctionTarget: (value: 1 | 2) => 1 | 2 = (value) => value;
let contextualFunctionResult = (contextualFunctionTarget = (value) => {
  const wrongContextualParameter: string = value; // error[TK2322]
  return value;
});
contextualFunctionResult(3); // error[TK2345]
