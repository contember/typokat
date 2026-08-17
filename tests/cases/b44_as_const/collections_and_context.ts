// Array literals become readonly fixed tuples. Mutable sources remain assignable to readonly
// targets; readonly const results are rejected by mutable call boundaries.

const frozenTuple = (["name", 1, { active: true }]) as const;
const exactTuple: readonly ["name", 1, { readonly active: true }] = frozenTuple;
const exactTupleName: "name" = frozenTuple[0];
const exactTupleCount: 1 = frozenTuple[1];
const exactTupleFlag: true = frozenTuple[2].active;

frozenTuple[0] = "name"; // error[TK2540]
frozenTuple[2].active = true; // error[TK2540]: Cannot assign to 'active' because it is a read-only property

declare function takesMutableTuple(value: ["name", 1, { active: boolean }]): void;
declare function takesMutableArray(value: number[]): void;

takesMutableTuple(frozenTuple); // error[TK2345]
const frozenNumbers = ([1, 2]) as const;
takesMutableArray(frozenNumbers); // error[TK2345]

const mutableTuple: ["name", 1] = ["name", 1];
const mutableArray: number[] = [1, 2];
const readonlyTupleControl: readonly ["name", 1] = mutableTuple;
const readonlyArrayControl: readonly number[] = mutableArray;

// `satisfies` supplies an outer contextual target. The nested const operand must ignore
// that context and retain its own exact readonly source type.
const noContextLeakObject = (({ kind: "left" }) as const) satisfies {
  readonly kind: "left" | "right";
};
const retainedContextFreeKind: "left" = noContextLeakObject.kind;

const noContextLeakTuple = ((["left", 1]) as const) satisfies readonly [
  "left" | "right",
  number,
];
const retainedContextFreeTupleKind: "left" = noContextLeakTuple[0];
const retainedContextFreeTupleCount: 1 = noContextLeakTuple[1];
