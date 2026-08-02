var globalThis: { invented: number }; // error[TK2397]: Declaration name conflicts with built-in global identifier 'globalThis'.

globalThis.invented; // error[TK2339]
