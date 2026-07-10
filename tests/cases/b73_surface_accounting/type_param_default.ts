// Surface-accounting spec (backlog 75). ENABLED by WU5 (review F3c): type-parameter
// DEFAULTS are record-only accounted — they are never lowered (deferred, divergences.md
// `constraints/type-parameter-defaults`), so an unresolved name inside a default was
// false-clean while tsc 6.0.3 --strict reports TS2304. Constraints (`extends`) ARE
// lowered and report normally (the control below).

// INCOMPLETE: an alias type-parameter default is not lowered.
type F<T = NoSuch> = T; // incomplete[annotation-lower/type-parameter-default/self]

// INCOMPLETE: an interface type-parameter default is not lowered.
interface G<T = NoSuch> { // incomplete[annotation-lower/type-parameter-default/self]
  v: T;
}

// INCOMPLETE: a class type-parameter default is not lowered.
class H<T = NoSuch> { // incomplete[annotation-lower/type-parameter-default/self]
  v!: T;
}

// CONTROL (supported): a constraint IS lowered — the unresolved name reports TK2304.
type K<T extends NoSuch2> = T; // error[TK2304]: Cannot find name 'NoSuch2'
