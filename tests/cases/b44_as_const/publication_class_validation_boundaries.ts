interface RetainedKeys { alpha: number; beta: string }
interface RetainedHolder { key: keyof RetainedKeys }
class RetainedBoundaries {
  nested = { outer: { key: "alpha" } } satisfies { outer: RetainedHolder }; // error[TK1360]
  array = [{ key: "alpha" }, { key: "beta" }] satisfies RetainedHolder[]; // error[TK1360] | error[TK1360]
  tuple = [{ key: "alpha" }, { key: "beta" }] satisfies [RetainedHolder, RetainedHolder]; // error[TK1360] | error[TK1360]
  distinct = {
    deferred: { key: "alpha" } satisfies RetainedHolder, // error[TK1360]
    ready: { key: "alpha" } satisfies { key: "alpha" | "beta" },
  };
  chained = ({ key: "alpha" } satisfies RetainedHolder) satisfies RetainedHolder; // error[TK1360] | error[TK1360]
  arrow = (() => ({ key: "alpha" })) satisfies () => RetainedHolder; // error[TK1360]
  contextualArrow = ((value) => ({ key: "alpha" })) satisfies (value: number) => RetainedHolder; // error[TK1360]
  explicitArrow = ((value: number) => ({ key: "alpha" })) satisfies (value: number) => RetainedHolder; // error[TK1360]
  declaredArrow = ((value: number): RetainedHolder => ({ key: "alpha" })) satisfies (value: number) => RetainedHolder;
  badBody = ((value: number): number => "bad") satisfies (value: number) => number; // error[TK2322]
  broad = { key: "alpha" } satisfies { key: string };
  ready = { key: "alpha" } satisfies { key: "alpha" | "beta" };
}
const retainedBoundaries = new RetainedBoundaries();
let readyBoundaryCopy = retainedBoundaries.ready.key;
readyBoundaryCopy = "beta"; // error[TK2322]
let distinctReadyCopy = retainedBoundaries.distinct.ready.key;
distinctReadyCopy = "beta"; // error[TK2322]
let broadBoundaryCopy = retainedBoundaries.broad.key;
broadBoundaryCopy = "beta";
