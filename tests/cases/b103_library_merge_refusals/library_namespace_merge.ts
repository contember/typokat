// Backlog 103 correctness tier. A user `namespace Intl` augmentation must merge into the
// library-owned namespace through the private collision epoch.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean — namespace merging is legal.
namespace Intl {
  export interface B103Extra {
    tag: string;
  }
}

declare const extra: Intl.B103Extra;
