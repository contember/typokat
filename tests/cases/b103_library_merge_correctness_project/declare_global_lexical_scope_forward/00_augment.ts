export {};

interface B103ModulePrivateShape {
  value: string;
}

declare global {
  interface B103DirectGlobalShape {
    direct: B103ModulePrivateShape;
  }

  namespace B103NestedGlobalNamespace {
    interface NestedShape {
      nested: B103ModulePrivateShape;
    }
  }

  var b103GlobalValue: B103ModulePrivateShape;
  function b103GlobalCall(value: B103ModulePrivateShape): B103ModulePrivateShape;
}
