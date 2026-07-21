export {};

const values: number[] = [1, 2, 3];
const mapped: string[] = values.map((value) => value + 1);
const promised: Promise<string> = Promise.resolve(1);
const matched: string = /typokat/i.test("Typokat");
const upper: number = "typokat".toUpperCase();
const domNode: number = document.createElement("div");
const formatted: number = new Intl.DateTimeFormat("en").format(new Date());

void [mapped, promised, matched, upper, domNode, formatted];
