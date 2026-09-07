declare const suppliedUseSeeds: [number, number, number];

declare class SuppliedUseFirst<A extends C | B, B extends { c: C }, C extends { b: B }> {
  constructor(values: [A, B, C]);
  value: [A, B, C];
}
const suppliedFirstUse = new SuppliedUseFirst(suppliedUseSeeds); // error[TK2345]
const suppliedFirstUseInvalid: { b: number } = suppliedFirstUse.value[1].c; // error[TK2322]
const suppliedFirstUseValid: { c: number } = suppliedFirstUse.value[2].b;

declare class SuppliedUseSecond<A extends B | C, B extends { c: C }, C extends { b: B }> {
  constructor(values: [A, B, C]);
  value: [A, B, C];
}
const suppliedSecondUse = new SuppliedUseSecond(suppliedUseSeeds); // error[TK2345]
const suppliedSecondUseValid: { b: number } = suppliedSecondUse.value[1].c;
const suppliedSecondUseInvalid: { c: number } = suppliedSecondUse.value[2].b; // error[TK2322]

declare const stillInvalidUseSeeds: [2, 1];
declare function stillInvalidUse<U extends T, T extends string>(values: [U, T]): [U, T];
const stillInvalidUseResult = stillInvalidUse(stillInvalidUseSeeds); // error[TK2345]
const stillInvalidUseValid: string = stillInvalidUseResult[0];
const stillInvalidUseWrong: number = stillInvalidUseResult[0]; // error[TK2322]

declare class StillInvalidUseBox<U extends T, T extends string> {
  constructor(values: [U, T]);
  value: [U, T];
}
const stillInvalidUseBox = new StillInvalidUseBox(stillInvalidUseSeeds); // error[TK2345]
const stillInvalidUseBoxValid: string = stillInvalidUseBox.value[0];
const stillInvalidUseBoxWrong: number = stillInvalidUseBox.value[0]; // error[TK2322]
