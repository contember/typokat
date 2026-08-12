import type { Mode } from "./source.js";

declare const mode: Mode;
const whole: Mode = mode;
const erasedValue = Mode.Read; // error[TK2304]
