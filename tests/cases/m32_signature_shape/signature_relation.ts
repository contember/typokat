// M32 - relation rules for optional and rest parameters. typokat keeps the
// existing strict contravariant parameter policy; these verdicts match tsc
// under --strictFunctionTypes. Function-type render details are code-only.

declare const required: (a: number, b: string) => void;
declare const optional: (a: number, b?: string) => void;
declare const rest: (a: number, ...b: string[]) => void;

const acceptsRequired: (a: number, b: string) => void = optional;
const rejectsOptionalTarget: (a: number, b?: string) => void = required; // error[TK2322]

const acceptsRestTarget: (a: number, ...b: string[]) => void = optional;
const rejectsOptionalFromRest: (a: number, b?: string) => void = rest; // error[TK2322]

const rejectsOptionalParamType: (a: number, b?: number) => void = optional; // error[TK2322]
const rejectsRestParamType: (a: number, ...b: number[]) => void = rest; // error[TK2322]
