// tsc 6.0.3 --strict: one TS2320 per independent base pair; inherited member
// compatibility requires identical types rather than one-way assignability.
interface A { first: number }
interface B { first: string }
interface D { second: boolean }
interface E { second: number }

interface C extends A, B {} // error[TK2320]: cannot simultaneously extend types 'A' and 'B' | error[TK2320]: cannot simultaneously extend types 'D' and 'E'
interface C extends D, E {}

interface LiteralX { x: 1 }
interface NumberX { x: number }
interface IdentityConflict extends LiteralX, NumberX {} // error[TK2320]: cannot simultaneously extend types 'LiteralX' and 'NumberX'
