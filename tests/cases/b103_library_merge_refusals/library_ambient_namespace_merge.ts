// Backlog 103 correctness tier. The ambient spelling of a legal namespace augmentation must merge
// into the library-owned `Intl` namespace through the private collision epoch.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean.
declare namespace Intl {
  interface B103Ambient {
    tag: string;
  }
}

declare const ambient: Intl.B103Ambient;
