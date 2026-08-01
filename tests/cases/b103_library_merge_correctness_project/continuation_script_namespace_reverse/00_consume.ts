declare const b103ContinuationShape: B103ContinuationSpace.Shape;
const continuationLabel: string = b103ContinuationShape.label;
const wrongContinuationLabel: number = b103ContinuationShape.label; // error[TK2322]: Type 'string' is not assignable to type 'number'

const continuationValue: number = B103ContinuationSpace.value;
const wrongContinuationValue: string = B103ContinuationSpace.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const continuationStaticRoot: unknown = B103ContinuationSpace.Box.root;

function readContinuationValue(): number {
  return B103ContinuationSpace.value;
}
const nestedContinuationValue: number = readContinuationValue();

type MissingContinuationChild = B103ContinuationSpace.Missing; // error[TK2694]: Namespace 'B103ContinuationSpace' has no exported member 'Missing'
type MissingContinuationRoot = B103MissingContinuationRoot.Member; // error[TK2503]: Cannot find namespace 'B103MissingContinuationRoot'
type ModuleOnlyNamespaceLeak = B103ModuleOnlySpace.Hidden; // error[TK2503]: Cannot find namespace 'B103ModuleOnlySpace'
