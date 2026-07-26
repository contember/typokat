// backlog 95 - every `retained_raw_walks.ts` shape, one level down.
//
// The raw argument walk is memoized per call region, so a call re-executed by a
// contextual re-walk of an *enclosing* argument gets its raw walk served from the memo
// instead of walked. A served walk produces no records, so wherever the raw walk is the
// only walk the memo must be undone: the argument is re-walked before the call frame
// closes. Every line below re-executes an inner call whose committed contextual walk
// declines, so a memo that is served and never recovered silently deletes all of them.
// This is `retained_raw_walks.ts`'s false-negative net, nested.
//
// The outer `run`/`wrap` is what re-executes the inner call: its argument is walked
// once raw and once by the committed contextual walk, so everything inside runs twice.
//
// tsc 6.0.3 --strict reports every diagnostic below at the same line and column. As in
// `retained_raw_walks.ts` it additionally reports TS7006 on the overload line;
// `TK7006` is in scope but not implemented yet (docs/reference/scope.md).

declare function run<T>(step: (value: number) => T): T;
declare function wrap<T>(value: { inner: T }): T;

declare function plain(step: (value: number) => void): void;
interface TwoCalls { (x: number): void; (x: string): void; }
declare function takesTwoCalls(cb: TwoCalls): void;
declare function over(x: number): void;
declare function over(x: string): void;
declare function wantsNumber(x: number): void;
declare function mix(first: never, second: (v: number) => void): void;
declare const nothing: never;
declare function pair(a: (v: number) => void, b: (v: number) => void): void;
declare function leadingSpread(a: number, cb: (v: number) => void): void;
declare const nums: number[];

// A generic arrow: the inner contextual walk returns before it enters the body.
const one = run(v0 => { plain(<U,>(v: number) => { undeclaredOne; }); return 0; }); // error[TK2304]: Cannot find name 'undeclaredOne'
// Two call signatures in the contextual position: no single signature to shape with.
const two = run(v0 => { takesTwoCalls(v => { undeclaredTwo; }); return 0; }); // error[TK2304]: Cannot find name 'undeclaredTwo'
// Overload resolution fails, so the inner committed argument check never runs at all.
const three = run(v0 => { over(v => { undeclaredThree; }); return 0; }); // error[TK2769] | error[TK2304]: Cannot find name 'undeclaredThree'
// A fresh object literal against a primitive parameter is never re-shaped.
const four = run(v0 => { wantsNumber({ k: undeclaredFour }); return 0; }); // error[TK2345] | error[TK2304]: Cannot find name 'undeclaredFour'
// A `never` parameter breaks the inner committed loop before the later arrow argument.
const five = run(v0 => { mix(nothing, v => { undeclaredFive; }); return 0; }); // error[TK2304]: Cannot find name 'undeclaredFive'
// Two superseded arguments in one inner call stay paired with their own parameters.
const six = run(v0 => { pair(v => { undeclaredSix; }, w => { undeclaredSeven; }); return 0; }); // error[TK2304]: Cannot find name 'undeclaredSix' | error[TK2304]: Cannot find name 'undeclaredSeven'
// A skipped spread argument shifts the callback's position, so the memo's recovery must
// re-walk by the SAME index the held effects used. The surrounding `TK2554`/`TK2345`/
// `incomplete` surface is the spread-argument deferral (owner 71), not part of this
// item's contract; the callback's single `TK2304` is.
const seven = run(v0 => { leadingSpread(...nums, v => { undeclaredEight; }); return 0; }); // error[TK2554] | error[TK2345] | error[TK2304]: Cannot find name 'undeclaredEight'  | incomplete[call/call-arguments/spread-argument]

// `new` and `super(...)` run the same argument machinery, including the recovery.
class Base { constructor(cb: (v: number) => void) { cb(1); } }
class Derived extends Base { constructor() { super(v => { undeclaredNine; }); } } // error[TK2304]: Cannot find name 'undeclaredNine'
const ten = run(v0 => { const made = new Base(v => { undeclaredTen; }); return 0; }); // error[TK2304]: Cannot find name 'undeclaredTen'

// A fresh-literal outer argument re-executes its subtree the same way an arrow does.
const eleven = wrap({ inner: run(v0 => { plain(<U,>(v: number) => { undeclaredEleven; }); return 0; }) }); // error[TK2304]: Cannot find name 'undeclaredEleven'
