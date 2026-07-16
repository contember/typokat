// tsc 6.0.3 accepts these declarations. Backlog 82 owns value-space publication from
// declare global, so the full-lib router must preserve an explicit non-permissive result.
export {};

declare global { // incomplete[decl/global-declaration/self]
  var B14DeferredGlobalValue: number;
  function B14DeferredGlobalFunction(value: number): number;
  class B14DeferredGlobalClass {
    value: number;
  }
}
