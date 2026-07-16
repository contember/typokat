// tsc 6.0.3 --strict --target es2025: TS2322 x4 below.

const element: HTMLDivElement = document.createElement("div");
const wrongElement: number = document.createElement("div"); // error[TK2322]

const response: Promise<Response> = fetch("https://example.invalid/");
const wrongResponse: Promise<string> = fetch("https://example.invalid/"); // error[TK2322]

const formatted: string = new Intl.DateTimeFormat("en").format(new Date());
const wrongFormatted: number = new Intl.DateTimeFormat("en").format(new Date()); // error[TK2322]: Type 'string' is not assignable to type 'number'

const closeCode: number = new CloseEvent("close", { code: 1000 }).code;
const wrongCloseCode: string = new CloseEvent("close", { code: 1000 }).code; // error[TK2322]: Type 'number' is not assignable to type 'string'
