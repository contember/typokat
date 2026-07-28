// Backlog 103 correctness: a fresh type slot can coexist with the library console value slot.
interface console {
  b103Probe: number;
}

declare const holder: console;
const probe: number = holder.b103Probe;
const wrongProbe: string = holder.b103Probe; // error[TK2322]: Type 'number' is not assignable to type 'string'
holder.b103Missing; // error[TK2339]
const wrongConsoleValue: string = console; // error[TK2322]
