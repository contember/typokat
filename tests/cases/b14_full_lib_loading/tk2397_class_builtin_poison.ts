// tsc 6.0.3 also reports TS2687 and TS2717 for the conflicting static declaration.
class globalThis { // error[TK2397]: Declaration name conflicts with built-in global identifier 'globalThis'.
  static Math: string;
}

globalThis.Math.abs(-1);
const mathString: string = globalThis.Math; // error[TK2322]
