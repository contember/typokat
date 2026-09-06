interface ConservativeKeys { alpha: number; beta: string }
export class ConservativeExternal {
  field = { key: "alpha" } satisfies { key: keyof ConservativeKeys }; // error[TK1360]
}
export class ConservativeBroad {
  field = { key: "alpha" } satisfies { key: string };
}
const conservativeExternal = new ConservativeExternal();
const conservativeBroad = new ConservativeBroad();
let conservativeBroadCopy = conservativeBroad.field.key;
conservativeBroadCopy = "beta";
const conservativeExternalRead: string = conservativeExternal.field.key;
