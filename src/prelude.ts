// typokat prelude (M28) — the built-in utility-type slice, checked as a REAL
// compilation unit before every user program (sprint 2026-07-05, backlog 12).
// Each alias is its ordinary lib.es5-style mapped/conditional definition, so the
// shared M25-M27 evaluator machinery does all the work (no second evaluator).
// Divergences from lib.es5.d.ts (documented in tests/cases/README.md):
//  - ReturnType uses `never[]` instead of lib.es5.d.ts's `any[]`, preserving
//    strict contravariant matching without introducing a permissive `any`.
//  - Record's value parameter is named V (cosmetic).
// The four string intrinsics are declared `= intrinsic` and seeded to their
// well-known marker ids by name (the prelude is a trusted asset; a user
// `= intrinsic` alias elsewhere is out of scope and degrades silently).
type Partial<T> = { [P in keyof T]?: T[P] };
type Required<T> = { [P in keyof T]-?: T[P] };
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Record<K extends keyof any, V> = { [P in K]: V };
type Exclude<T, U> = T extends U ? never : T;
type Extract<T, U> = T extends U ? T : never;
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type NonNullable<T> = T extends null | undefined ? never : T;
type ReturnType<T> = T extends (...args: never[]) => infer R ? R : never;
type Uppercase<S extends string> = intrinsic;
type Lowercase<S extends string> = intrinsic;
type Capitalize<S extends string> = intrinsic;
type Uncapitalize<S extends string> = intrinsic;
