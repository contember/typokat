// Recursive alias applications are transparent structural types. Productive
// recursion stays lazy; direct cycles still report TK2456. Cross-checked with
// TypeScript 6.0.3 under --strict --noEmit.

type ArrayRecursive = ArrayRecursive[];
declare const arrayRecursive: ArrayRecursive;
const arrayView: ArrayRecursive[] = arrayRecursive;
const arrayBack: ArrayRecursive = arrayView;
const arrayWrong: string = arrayRecursive; // error[TK2322]: not assignable
const arrayEdgeWrong: string = arrayRecursive[0]; // error[TK2322]: not assignable

type TupleRecursive = [TupleRecursive];
declare const tupleRecursive: TupleRecursive;
const tupleView: [TupleRecursive] = tupleRecursive;
const tupleBack: TupleRecursive = tupleView;
const tupleWrong: string = tupleRecursive; // error[TK2322]: not assignable
const tupleEdgeWrong: string = tupleRecursive[0]; // error[TK2322]: not assignable

type FunctionRecursive = () => FunctionRecursive;
declare const functionRecursive: FunctionRecursive;
const functionView: () => FunctionRecursive = functionRecursive;
const functionBack: FunctionRecursive = functionView;
const functionWrong: string = functionRecursive; // error[TK2322]: not assignable
const functionEdgeWrong: string = functionRecursive(); // error[TK2322]: not assignable

type MutualLeft = { right: MutualRight };
type MutualRight = [MutualLeft];
declare const mutualLeft: MutualLeft;
const mutualRight: MutualRight = mutualLeft.right;
const mutualBack: MutualLeft = mutualRight[0];
const mutualWrong: string = mutualRight[0]; // error[TK2322]: not assignable

type GenericRecursive<T> = T | GenericRecursive<T>[];
declare const stringTree: GenericRecursive<string>;
declare const numberTree: GenericRecursive<number>;
const stringTreeView: string | GenericRecursive<string>[] = stringTree;
const stringTreeBack: GenericRecursive<string> = stringTreeView;
const badStringTree: GenericRecursive<string> = [1]; // error[TK2322]: not assignable
const wrongNumberTree: GenericRecursive<number> = stringTree; // error[TK2322]: not assignable
const wrongStringTree: GenericRecursive<string> = numberTree; // error[TK2322]: not assignable

// Recursion in a conditional branch is productive. The check surface remains
// the direct-cycle boundary; descending through the true branch must not report
// TK2456 (the same guarded shape used by the default library's Awaited alias).
type ConditionalRecursive<T> = T extends { value: infer U }
	? ConditionalRecursive<U>
	: T;
declare const conditionalRecursive: ConditionalRecursive<{
	value: { value: string };
}>;
const conditionalRecursiveView: string = conditionalRecursive;
const conditionalRecursiveWrong: number = conditionalRecursive; // error[TK2322]: not assignable

type NonRegular<T> = [T, NonRegular<T[]>];
declare const nonRegular: NonRegular<string>;
const nonRegularView: [string, NonRegular<string[]>] = nonRegular;
const nonRegularBack: NonRegular<string> = nonRegularView;
const nonRegularHead: string = nonRegular[0];
const nonRegularTail: NonRegular<string[]> = nonRegular[1];
const nonRegularWrong: number = nonRegular[0]; // error[TK2322]: not assignable
const nonRegularTailWrong: NonRegular<number[]> = nonRegular[1]; // error[TK2322]: not assignable

type DirectCycle = DirectCycle; // error[TK2456]: circularly references itself
type MutualCycleA = MutualCycleB; // error[TK2456]: circularly references itself
type MutualCycleB = MutualCycleA; // error[TK2456]: circularly references itself
