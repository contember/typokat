// Supported projections, iteration bindings, and straight-line assignments retain const
// provenance. Branch-local assignment provenance does not leak through a join.

const projectionSource = { value: 1 as const };
let { value: projectedValue } = projectionSource;
let projectedCopy = projectedValue;
projectedCopy = 2; // error[TK2322]: Type '2' is not assignable to type '1'

for (let item of [1 as const]) {
  let loopCopy = item;
  loopCopy = 2; // error[TK2322]: Type '2' is not assignable to type '1'
}

let flowTarget: 1 | 2 = 2;
flowTarget = 1 as const;
let flowCopy = flowTarget;
flowCopy = 2; // error[TK2322]: Type '2' is not assignable to type '1'

declare const condition: boolean;
let joinedTarget: 1 | 2 = 2;
if (condition) {
  joinedTarget = 1 as const;
}
let joinedCopy = joinedTarget;
joinedCopy = 2;
