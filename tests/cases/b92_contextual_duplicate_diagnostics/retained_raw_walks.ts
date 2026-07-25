// backlog 92 - the raw argument walk still reports wherever the committed contextual
// walk declines to re-walk the argument.
//
// Exactly one of the two walks an argument gets may report, and the committed walk
// wins whenever it runs, because it is the one that sees the instantiated contextual
// target. Every line below is a case where it does NOT run, so dropping the raw walk
// unconditionally - the obvious wrong way to remove the duplicates - would silently
// delete all of these. This fixture is that false-negative net.
//
// tsc 6.0.3 --strict reports every diagnostic below at the same line and column. It
// additionally reports TS7006 ("parameter implicitly has an 'any' type") on the
// overload line; `TK7006` is in scope but not implemented yet
// (docs/reference/scope.md), so that line pins only what typokat models.

declare function plain(step: (value: number) => void): void;
interface TwoCalls { (x: number): void; (x: string): void; }
declare function takesTwoCalls(cb: TwoCalls): void;
declare function over(x: number): void;
declare function over(x: string): void;
declare function wantsNumber(x: number): void;
declare function mix(first: never, second: (v: number) => void): void;
declare const nothing: never;
declare function pair(a: (v: number) => void, b: (v: number) => void): void;
declare const nums: number[];
declare function leadingSpread(a: number, cb: (v: number) => void): void;

// A generic arrow: the contextual walk returns before it enters the body.
plain(<U,>(v: number) => { undeclaredOne; }); // error[TK2304]: Cannot find name 'undeclaredOne'
// Two call signatures in the contextual position: no single signature to shape with.
takesTwoCalls(v => { undeclaredTwo; }); // error[TK2304]: Cannot find name 'undeclaredTwo'
// Overload resolution fails, so the committed argument check never runs at all.
over(v => { undeclaredThree; }); // error[TK2769] | error[TK2304]: Cannot find name 'undeclaredThree'
// A fresh object literal against a primitive parameter is never re-shaped.
wantsNumber({ k: undeclaredFour }); // error[TK2345] | error[TK2304]: Cannot find name 'undeclaredFour'
// A `never` parameter breaks the committed loop before the later arrow argument.
mix(nothing, v => { undeclaredFive; }); // error[TK2304]: Cannot find name 'undeclaredFive'
// Two superseded arguments in one call stay paired with their own parameters.
pair(v => { undeclaredSix; }, w => { undeclaredSeven; }); // error[TK2304]: Cannot find name 'undeclaredSix' | error[TK2304]: Cannot find name 'undeclaredSeven'
// A skipped spread argument shifts the callback's position, so the held effects and
// the parameter targets must stay aligned by the SAME index. The surrounding
// `TK2554`/`TK2345`/`incomplete` surface is the spread-argument deferral (owner 71),
// not part of this item's contract; the callback's single `TK2304` is.
leadingSpread(...nums, v => { undeclaredEight; }); // error[TK2554] | error[TK2345] | error[TK2304]: Cannot find name 'undeclaredEight' | incomplete[call/call-arguments/spread-argument]

// `new` and `super(...)` run the same argument machinery.
class Base { constructor(cb: (v: number) => void) { cb(1); } }
class Derived extends Base { constructor() { super(v => { undeclaredNine; }); } } // error[TK2304]: Cannot find name 'undeclaredNine'
const made = new Base(v => { undeclaredTen; }); // error[TK2304]: Cannot find name 'undeclaredTen'
