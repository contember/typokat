// Backlog 103 correctness: the ambient namespace spelling reaches the same merged identity.
declare namespace Intl {
  interface B103Ambient {
    enabled: boolean;
  }
}

declare const ambient: Intl.B103Ambient;
const enabled: boolean = ambient.enabled;
const wrongEnabled: string = ambient.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'string'
