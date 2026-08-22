export {};

declare global {
  interface Array<T> {
    fromDeclareGlobal(): readonly T[];
  }
}
