// A value import must not hide an ambient type in the other declaration space.
// Cross-checked with tsc 6.0.3 --strict.

import { Partial } from "./a";

const value: number = Partial;
const partial: Partial<{ ready: number }> = {};
