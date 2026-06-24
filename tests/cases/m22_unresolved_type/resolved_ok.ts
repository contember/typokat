// M22 — references that DO resolve must NOT report TK2304 (no false positives).

// Forward reference: a type used before its declaration still resolves (top-level type
// declarations are hoisted by the binder), so no TK2304.
const fwd: Later = 1;
type Later = number;

// A type parameter in scope is not an unresolved name.
function gen<T>(x: T): T[] { return [x]; }

// The built-in `Array<T>` resolves without a lib.
const xs: Array<number> = [];

// `Array` is a recognized built-in, so a bare `Array` or a wrong type-argument count
// is tsc's TS2314 ("Generic type 'Array<T>' requires 1 type argument(s)") — a DEFERRED
// arity check, NOT "cannot find name". typokat emits no TK2304 here; the reference
// degrades to the error type. (Documented divergence; see README.)
type BareArray = Array;
type WrongArity = Array<number, string>;

// Declared interface / alias / class all resolve, in every position.
interface Pt { x: number; }
type PtAlias = Pt;
class C { n: number = 0; }
const p: Pt = { x: 1 };
const pa: PtAlias = { x: 1 };
const c: C = new C();

// A value used as a TYPE is tsc's TS2749 ("'val' refers to a value, but is being used
// as a type here") — a DISTINCT diagnostic that typokat defers. The name DOES resolve
// (in the value space), so it is NOT "cannot find name": typokat emits no TK2304 and
// the reference degrades to the error type. (Documented divergence; see README.)
const val = 1;
const vt: val = 2;
