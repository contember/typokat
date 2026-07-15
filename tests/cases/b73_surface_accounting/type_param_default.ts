// Surface-accounting spec (backlog 75). WU3 lowers alias and interface defaults through
// immutable type-group endpoints. Class defaults remain conservatively incomplete.

// WU3: alias defaults lower into the immutable type-group endpoint.
type F<T = NoSuch> = T; // error[TK2304]: Cannot find name 'NoSuch'

// WU3: interface defaults lower into the immutable type-group endpoint.
interface G<T = NoSuch> { // error[TK2304]: Cannot find name 'NoSuch'
  v: T;
}

// INCOMPLETE: a class type-parameter default is not lowered.
class H<T = NoSuch> { // incomplete[annotation-lower/type-parameter-default/self]
  v!: T;
}

// CONTROL (supported): a constraint IS lowered — the unresolved name reports TK2304.
type K<T extends NoSuch2> = T; // error[TK2304]: Cannot find name 'NoSuch2'
