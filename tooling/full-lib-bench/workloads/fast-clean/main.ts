export {};

const values: number[] = [1, 2, 3];
const mapped: number[] = values.map((value) => value + 1);
const tuple: readonly [string, number] = ["answer", 42];
const promised: Promise<number> = Promise.resolve(mapped[0]);
const matched: boolean = /typokat/i.test("Typokat");
const upper: string = tuple[0].toUpperCase();
const fixed: string = tuple[1].toFixed(1);
const domNode: HTMLDivElement = document.createElement("div");
const formatted: string = new Intl.DateTimeFormat("en").format(new Date());

declare const generator: Generator<number, void, unknown>;
const iterator: Iterator<number> = generator;
const next: IteratorResult<number, void> = generator.next();
void [promised, matched, upper, fixed, domNode, formatted, iterator, next];
