// tsc 6.0.3 --strict: TS2669; global augmentation requires an external-module context.
declare global { // error[TK2669]: Augmentations for the global scope can only be directly nested in external modules or ambient module declarations
  interface InvalidScriptGlobal { value: number }
}
