// tsc 6.0.3 --strict --noEmit: only the seven TS2503 namespace errors below.
declare const computedProperty: "computedProperty";
declare const computedMethod: "computedMethod";
declare const computedSibling: "computedSibling";

namespace AvailableInterface {
  export interface Good {}
}

interface InterfaceTraversal {
  [computedProperty]: MissingProperty.Child; // incomplete[signature/property-signature/computed-key]: computed property signature key not visited | error[TK2503]: Cannot find namespace 'MissingProperty'
  [computedMethod]<Generic extends MissingMethod.Constraint>(value: MissingMethod.Parameter): MissingMethod.Return; // incomplete[signature/method-signature/computed-key]: computed method signature key not visited | error[TK2503]: Cannot find namespace 'MissingMethod' | error[TK2503]: Cannot find namespace 'MissingMethod' | error[TK2503]: Cannot find namespace 'MissingMethod'
  [computedSibling]: AvailableInterface.Good | MissingComputedSibling.Child; // incomplete[signature/property-signature/computed-key]: computed property signature key not visited | incomplete[annotation-lower/type-name/qualified-name]: qualified type path classified; leaf lowering deferred to WU3 | error[TK2503]: Cannot find namespace 'MissingComputedSibling'
  later: MissingLater.Child; // error[TK2503]: Cannot find namespace 'MissingLater'
}

interface HeritageTraversal extends AvailableInterface.Good, MissingHeritage.Child {} // incomplete[annotation-lower/type-name/qualified-name]: qualified type path classified; leaf lowering deferred to WU3 | error[TK2503]: Cannot find namespace 'MissingHeritage'
