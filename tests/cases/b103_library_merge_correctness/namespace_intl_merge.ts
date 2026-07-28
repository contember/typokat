// Backlog 103 correctness: a namespace reopening adds its member and retains native Intl values.
namespace Intl {
  export interface B103Extra {
    tag: string;
  }
}

declare const extra: Intl.B103Extra;
const tag: string = extra.tag;
const wrongTag: number = extra.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const formatted: string = new Intl.NumberFormat("en").format(1);
