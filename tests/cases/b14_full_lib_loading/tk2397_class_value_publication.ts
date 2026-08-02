class globalThis { // error[TK2397]: Declaration name conflicts with built-in global identifier 'globalThis'.
  static invented: number;
}

const classInvented: number = globalThis.invented;
const absolute: number = globalThis.Math.abs(-1);
