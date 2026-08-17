// tsc 6.0.3 --strict: non-fresh structured sources are checked as whole values, so
// their property/element/return reason chains remain nested under TS1360.

declare const object: { value: string };
object satisfies { value: number }; // error[TK1360]

declare const tuple: [string];
tuple satisfies [number]; // error[TK1360]

declare const callback: (value: number) => string;
callback satisfies (value: number) => number; // error[TK1360]

interface RecursiveSource { value: string; next?: RecursiveSource }
interface RecursiveTarget { value: number; next?: RecursiveTarget }
declare const recursive: RecursiveSource;
recursive satisfies RecursiveTarget; // error[TK1360]
