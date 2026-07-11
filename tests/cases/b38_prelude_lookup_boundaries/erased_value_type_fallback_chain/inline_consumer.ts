// Cross-checked with tsc 6.0.3 --strict.

import type { Partial } from "./inline_middle";

type Inline = Partial<{ value: string }>;
const inline: Inline = {};
