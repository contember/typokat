// Backlog 103 correctness tier, split shape. Two files each merge into a different library-owned
// group; both augmentations must be visible regardless of their separate source files.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` accepts both merges cleanly.
interface Window {
  b103SplitFlag: boolean;
}
