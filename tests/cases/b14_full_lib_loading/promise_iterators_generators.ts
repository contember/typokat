// tsc 6.0.3 --strict --target es2025: TS2322 x2 below. Iterator/generator semantic
// acceptance remains census-gated; the yield expression itself stays explicitly unavailable.

const promised: Promise<number> = Promise.resolve(1).then((value) => value + 1);
const wrongPromise: Promise<string> = Promise.resolve(1); // error[TK2322]
const incrementedPromise = Promise.resolve(1).then((value) => value + 1);
const thenResult: Promise<number> = incrementedPromise;
const wrongThen: Promise<string> = incrementedPromise; // error[TK2322]

function* b14YieldBoundary() {
  yield 1; // incomplete[expr-infer/yield-expression/self]
}
