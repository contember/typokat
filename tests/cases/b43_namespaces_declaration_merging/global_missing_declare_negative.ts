// tsc 6.0.3 --strict: TS2670; an invalid augmentation must not publish its body.
export {};

global { // error[TK2670]: Augmentations for the global scope should have 'declare' modifier unless they appear in already ambient context
  interface InvalidUndeclaredGlobal { value: number }
}

declare const invalidUndeclaredGlobal: InvalidUndeclaredGlobal; // error[TK2304]: Cannot find name 'InvalidUndeclaredGlobal'
