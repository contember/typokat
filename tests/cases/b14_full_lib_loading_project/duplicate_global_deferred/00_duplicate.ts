// tsc reports TS2451 on both declarations. Backlog 18 remains the owner; the full-lib
// private path must not silently select one duplicate namespace payload.
namespace B14DuplicateGlobal {
  export const value: number = 1;
  export const value: string = "duplicate"; // incomplete[decl/variable-declaration/namespace-payload-duplicate-value]
}
