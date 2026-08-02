// tsc 6.0.3 --strict --target es2025: TS2397 + inner TS2304.
declare namespace globalThis { // error[TK2397]: Declaration name conflicts with built-in global identifier 'globalThis'.
  type Missing = DoesNotExist; // error[TK2304]: Cannot find name 'DoesNotExist'
}
