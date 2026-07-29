// The second half of the split merge; see 00_window.ts for the oracle. The `Window` read is an
// annotation, not the global `window` value (see library_interface_window_merge.ts).
interface String {
  b103SplitUpper(): string;
}

declare const view: Window;
const splitFlag: boolean = view.b103SplitFlag;
const splitUpper: string = "quiet".b103SplitUpper();
