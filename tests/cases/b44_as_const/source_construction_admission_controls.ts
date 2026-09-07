const futureWriteResult = futureWriteSource();
declare function futureWriteSource<T extends string>(): number;
const immutableWriteCopy = futureWriteSource;
let mutableWriteCopy = immutableWriteCopy;
mutableWriteCopy = immutableWriteCopy;
const futureWriteWrong: string = futureWriteResult; // error[TK2322]

const futureEscapeResult = futureEscapeSource();
declare function futureEscapeSource<T extends string>(): number;
const escapedCallable = { callable: futureEscapeSource };
const escapedCallableResult: number = escapedCallable.callable();
const futureEscapeWrong: string = futureEscapeResult; // error[TK2322]

const futureCaptureResult = futureCaptureSource();
declare function futureCaptureSource<T extends string>(): number;
const capturedCallable = () => futureCaptureSource();
const capturedCallableResult: number = capturedCallable();
const futureCaptureWrong: string = futureCaptureResult; // error[TK2322]

declare const constructionChoice: boolean;
const futureUnionResult = futureUnionSource();
declare function futureUnionSource<T extends string>(): number;
declare function otherUnionSource<T extends string>(): number;
const unclosedUnion = constructionChoice ? futureUnionSource : otherUnionSource;
const futureUnionWrong: string = futureUnionResult; // error[TK2322]

const futureOverloadResult = futureOverloadSource(1);
declare function futureOverloadSource<T extends string>(value: number): number;
declare function futureOverloadSource<T extends string>(value: string): number;
const futureOverloadWrong: string = futureOverloadResult; // error[TK2322]
