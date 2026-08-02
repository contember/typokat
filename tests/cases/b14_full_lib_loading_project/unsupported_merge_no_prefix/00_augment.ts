// The bigint literal annotation makes this Array augmentation unavailable. The private compiler must
// withhold the complete merged surface instead of accepting the frozen library prefix.
interface Array<T> {
  b14Unsupported(value: 1n): T; // incomplete[annotation-lower/literal-type/bigint]
}
