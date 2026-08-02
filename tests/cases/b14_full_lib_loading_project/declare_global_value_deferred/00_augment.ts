// tsc 6.0.3 accepts these declarations. The production full-library route must publish every
// value-space declaration through the global target while retaining this module's lexical scope.
export {};

declare global {
  var B14DeferredGlobalValue: number;
  function B14DeferredGlobalFunction(value: number): number;
  class B14DeferredGlobalClass {
    value: number;
  }
}
