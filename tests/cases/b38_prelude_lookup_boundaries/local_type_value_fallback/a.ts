// A type-only local Math interface leaves the ambient value slot reachable.
// Cross-checked with tsc 6.0.3 --strict.

export {};

interface Math {
  local: boolean;
}

const typed: Math = { local: true };
const absolute: number = Math.abs(-1);
