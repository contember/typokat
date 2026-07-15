// tsc 6.0.3 --strict: TS2694 x32 and TS2503 x9 across the measured routes.
namespace N {}

type UnionContext = N.UnionLeft | N.UnionRight; // error[TK2694]: Namespace 'N' has no exported member 'UnionLeft' | error[TK2694]: Namespace 'N' has no exported member 'UnionRight'
type IntersectionContext = N.IntersectionLeft & N.IntersectionRight; // error[TK2694]: Namespace 'N' has no exported member 'IntersectionLeft' | error[TK2694]: Namespace 'N' has no exported member 'IntersectionRight'
type TupleContext = [N.TupleFirst, N.TupleSecond]; // error[TK2694]: Namespace 'N' has no exported member 'TupleFirst' | error[TK2694]: Namespace 'N' has no exported member 'TupleSecond'
type IndexedAccessContext = N.IndexedObject[N.IndexedKey]; // error[TK2694]: Namespace 'N' has no exported member 'IndexedObject' | error[TK2694]: Namespace 'N' has no exported member 'IndexedKey'
type ConditionalContext = N.Check extends N.Extends ? N.TrueBranch : N.FalseBranch; // error[TK2694]: Namespace 'N' has no exported member 'Check' | error[TK2694]: Namespace 'N' has no exported member 'Extends' | error[TK2694]: Namespace 'N' has no exported member 'TrueBranch' | error[TK2694]: Namespace 'N' has no exported member 'FalseBranch'
type MappedContext = { [K in keyof N.MappedKeys]: N.MappedValue }; // error[TK2694]: Namespace 'N' has no exported member 'MappedKeys' | error[TK2694]: Namespace 'N' has no exported member 'MappedValue'
type FunctionContext = (value: N.FunctionParameter) => N.FunctionReturn; // error[TK2694]: Namespace 'N' has no exported member 'FunctionParameter' | error[TK2694]: Namespace 'N' has no exported member 'FunctionReturn'
type ConstructorContext = new (value: N.ConstructorParameter) => N.ConstructorReturn; // error[TK2694]: Namespace 'N' has no exported member 'ConstructorParameter' | error[TK2694]: Namespace 'N' has no exported member 'ConstructorReturn'
type TemplateContext = `${N.TemplateFirst}${N.TemplateSecond}`; // error[TK2694]: Namespace 'N' has no exported member 'TemplateFirst' | error[TK2694]: Namespace 'N' has no exported member 'TemplateSecond'
type CallSignatureContext = { (value: N.CallParameter): N.CallReturn }; // error[TK2694]: Namespace 'N' has no exported member 'CallParameter' | error[TK2694]: Namespace 'N' has no exported member 'CallReturn'
type ConstructSignatureContext = { new (value: N.ConstructParameter): N.ConstructReturn }; // error[TK2694]: Namespace 'N' has no exported member 'ConstructParameter' | error[TK2694]: Namespace 'N' has no exported member 'ConstructReturn'
type MethodSignatureContext = {
  method<T extends N.SignatureConstraint>(value: N.SignatureParameter): N.SignatureReturn; // error[TK2694]: Namespace 'N' has no exported member 'SignatureConstraint' | error[TK2694]: Namespace 'N' has no exported member 'SignatureParameter' | error[TK2694]: Namespace 'N' has no exported member 'SignatureReturn'
};

abstract class ClassContext<T extends N.ClassConstraint> { // error[TK2694]: Namespace 'N' has no exported member 'ClassConstraint'
  abstract field: N.ClassField; // error[TK2694]: Namespace 'N' has no exported member 'ClassField'
  abstract method<U extends N.MethodConstraint>(value: N.MethodParameter): N.MethodReturn; // error[TK2694]: Namespace 'N' has no exported member 'MethodConstraint' | error[TK2694]: Namespace 'N' has no exported member 'MethodParameter' | error[TK2694]: Namespace 'N' has no exported member 'MethodReturn'
}

namespace Available {
  export interface Good {}
}
type UnionUnavailableFirst = Available.Good | MissingRoot.Bad; // error[TK2503]: Cannot find namespace 'MissingRoot'
type FunctionUnavailableFirst = (value: Available.Good) => MissingReturn.Bad; // error[TK2503]: Cannot find namespace 'MissingReturn'

enum DeferredEnum { A } // incomplete[decl/enum-declaration/self]
type EnumUnavailableFirst = DeferredEnum.A | MissingEnumSibling.Bad; // error[TK2503]: Cannot find namespace 'MissingEnumSibling'

class GenericBase<T> {}
class QualifiedHeritage extends GenericBase<MissingHeritage.Root> {} // incomplete[class/class-heritage/type-arguments] | error[TK2503]: Cannot find namespace 'MissingHeritage'

declare const computedClassKey: "field";
class ComputedClassField {
  [computedClassKey]!: MissingField.Root; // incomplete[class/property-definition/computed-key] | error[TK2503]: Cannot find namespace 'MissingField'
}

class GenericHeaderOrder<
  First extends MissingHeader.C1 = MissingHeader.D1, // error[TK2503]: Cannot find namespace 'MissingHeader' | error[TK2503]: Cannot find namespace 'MissingHeader'
  Second extends MissingHeader.C2 = MissingHeader.D2, // error[TK2503]: Cannot find namespace 'MissingHeader' | error[TK2503]: Cannot find namespace 'MissingHeader'
> {}
