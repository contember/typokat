// tsc 6.0.3 --strict --noEmit: declarations are clean; only consumer TS2322 witnesses remain.
export {};

declare global {
  function b103AmbientOverload(value: string): number;
  function b103AmbientOverload(value: number): string;
}
