// tsc 6.0.3 --strict: only the final number assignment reports TS2322.
// The true branch walks the deep function metadata beside its infer-bearing object.

type RewriteMeta00 = unknown;
type RewriteMeta01 = <P extends RewriteMeta00>() => P;
type RewriteMeta02 = <P = RewriteMeta01>() => P;
type RewriteMeta03 = <P extends RewriteMeta02>() => P;
type RewriteMeta04 = <P = RewriteMeta03>() => P;
type RewriteMeta05 = <P extends RewriteMeta04>() => P;
type RewriteMeta06 = <P = RewriteMeta05>() => P;
type RewriteMeta07 = <P extends RewriteMeta06>() => P;
type RewriteMeta08 = <P = RewriteMeta07>() => P;
type RewriteMeta09 = <P extends RewriteMeta08>() => P;
type RewriteMeta10 = <P = RewriteMeta09>() => P;
type RewriteMeta11 = <P extends RewriteMeta10>() => P;
type RewriteMeta12 = <P = RewriteMeta11>() => P;
type RewriteMeta13 = <P extends RewriteMeta12>() => P;
type RewriteMeta14 = <P = RewriteMeta13>() => P;
type RewriteMeta15 = <P extends RewriteMeta14>() => P;
type RewriteMeta16 = <P = RewriteMeta15>() => P;

type RewriteDeep<T> =
    T extends { value: infer U } ? RewriteMeta16 & { value: U } : never;
type RewrittenMetadata = RewriteDeep<{ value: "leaf" }>;
declare const rewrittenMetadata: RewrittenMetadata;
const preservedRewrite: "leaf" = rewrittenMetadata.value;
const rejectedRewrite: number = rewrittenMetadata.value; // error[TK2322]
