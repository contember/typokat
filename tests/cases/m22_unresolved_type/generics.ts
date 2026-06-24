// M22 — TK2304 through generic type references, the `Array<T>` built-in, and `keyof`.

// An unresolved generic NAME reports TK2304 on the name itself.
type G = Vanished<number>;            // error[TK2304]: Cannot find name 'Vanished'

// An unresolved type ARGUMENT to a resolved generic reports TK2304 on the argument.
interface Box<T> { value: T; }
type B = Box<Absent>;                 // error[TK2304]: Cannot find name 'Absent'

// `Array<T>` is the built-in (no lib needed); an unresolved element argument is still
// TK2304 on the argument.
type AB = Array<Nope>;                // error[TK2304]: Cannot find name 'Nope'

// Even when the generic NAME is unresolved, an unresolved ARGUMENT is still reported
// (the arguments are lowered for their diagnostics before degrading to the error type).
type GG = Lost<AlsoGone>;             // error[TK2304]: Cannot find name 'Lost' | error[TK2304]: Cannot find name 'AlsoGone'

// `keyof` of an unresolved operand reports TK2304 on the operand.
type K = keyof Unk;                   // error[TK2304]: Cannot find name 'Unk'
