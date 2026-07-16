// tsc 6.0.3 --strict --target es2025 reports TS2552: IteratorObjectConstructor is
// library-module-local, not a global type. Typokat intentionally normalizes the missing name to
// TK2304; this does not claim Iterator/IteratorObject semantic usability.
export {};

declare const leakedIteratorConstructor: IteratorObjectConstructor; // error[TK2304]: Cannot find name 'IteratorObjectConstructor'
