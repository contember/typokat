// Backlog 103 correctness tier. A primitive wrapper interface augmentation must merge into the
// library-owned `String` group through the private collision epoch.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean — the merge is legal.
interface String {
  b103Upper(): string;
}

const shouted: string = "quiet".b103Upper();
