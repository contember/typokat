// tsc 6.0.3 --strict infers unknown and reports only TS2322 below.

type RecursionDepth = 0 | 1;
type NextRecursionDepth<D extends RecursionDepth> = D extends 0 ? 1 : 1;
type SourceAtDepth<D extends RecursionDepth> = D extends 1 ? string : number;
type TargetAtDepth<D extends RecursionDepth, T> = D extends 1 ? T : number;

interface RecursionDepthSource<D extends RecursionDepth> {
  value: SourceAtDepth<D>;
  next: RecursionDepthSource<NextRecursionDepth<D>>;
}

interface RecursionDepthTarget<D extends RecursionDepth, T> {
  value: TargetAtDepth<D, T>;
  next: RecursionDepthTarget<NextRecursionDepth<D>, T>;
}

declare const recursionDepthSource: RecursionDepthSource<0>;
declare function inferAtRecursionCutoff<T>(value: RecursionDepthTarget<0, T>): T;

const inferredAtRecursionCutoff = inferAtRecursionCutoff(recursionDepthSource);
const recursionDepthOracle: string = inferredAtRecursionCutoff; // error[TK2322]: Type 'unknown' is not assignable to type 'string'
