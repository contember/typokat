// The second half of the split merge; see 00_window.ts for the oracle. The `Window` read is an
// annotation, not the global `window` value (see library_interface_window_merge.ts).
interface String { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  b103SplitUpper(): string;
}

declare const view: Window;
const splitFlag: boolean = view.b103SplitFlag; // error[TK2339]: Property 'b103SplitFlag' does not exist
const splitUpper: string = "quiet".b103SplitUpper(); // error[TK2339]: Property 'b103SplitUpper' does not exist
