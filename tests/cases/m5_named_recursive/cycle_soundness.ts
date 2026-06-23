// M5 soundness — the §6.3 cache-poisoning hazard. Relating CA<:AA decides the
// nested CB<:AB *provisionally* true (under the in-flight (CA,AA) assumption); that
// provisional `true` must NEVER reach the durable cache, or a later independent
// CB<:AB query would read a spurious `true` and DROP a real error. Both the
// enclosing CA<:AA and the standalone CB<:AB assignments must be reported, and the
// verdict must be independent of statement order — so the standalone failing
// assignment appears BOTH before and after the enclosing one (distinct names).

interface AA { peer: AB; tag: number; }
interface AB { back: AA; leaf: number; }
interface CA { peer: CB; tag: string; }   // tag differs from AA -> CA<:AA is false
interface CB { back: CA; leaf: number; }   // CB<:AB is genuinely false (via back)

declare const ca: CA;
declare const cb: CB;

// 1. standalone failing assignment FIRST (cold cache) — must error.
const standaloneFirst: AB = cb; // error[TK2322]

// 2. the enclosing query that provisionally relates CB<:AB while deciding CA<:AA.
const enclosing: AA = ca;       // error[TK2322]

// 3. standalone failing assignment AGAIN, AFTER the enclosing query ran — must
//    STILL error (this is the one a poisoned cache would silently drop).
const standaloneAfter: AB = cb; // error[TK2322]
