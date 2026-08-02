// Test-only utility aliases and bounded ambient values for checker unit tests.
type Array<T> = T[];
type Partial<T> = { [P in keyof T]?: T[P] };
type Required<T> = { [P in keyof T]-?: T[P] };
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Record<K extends keyof any, V> = { [P in K]: V };
type Exclude<T, U> = T extends U ? never : T;
type Extract<T, U> = T extends U ? T : never;
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type NonNullable<T> = T extends null | undefined ? never : T;
type ReturnType<T extends (...args: never[]) => unknown> = T extends (...args: never[]) => infer R ? R : never;
type ThisParameterType<T> = T extends (this: infer U, ...args: never[]) => unknown ? U : unknown;
type OmitThisParameter<T> = intrinsic;
type Uppercase<S extends string> = intrinsic;
type Lowercase<S extends string> = intrinsic;
type Capitalize<S extends string> = intrinsic;
type Uncapitalize<S extends string> = intrinsic;
type ThisType<T> = intrinsic;

declare const console: {
  log(...data: unknown[]): void;
  warn(...data: unknown[]): void;
  error(...data: unknown[]): void;
};

declare const Math: {
  abs(x: number): number;
  ceil(x: number): number;
  floor(x: number): number;
  max(...values: number[]): number;
  min(...values: number[]): number;
  round(x: number): number;
  random(): number;
};
