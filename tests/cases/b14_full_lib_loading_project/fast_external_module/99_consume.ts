// tsc 6.0.3 --strict --target es2025: the project has TS2339 x5 in 00_api.ts and
// TS2322 below. Both files are external modules, so no user declaration contributes
// to the library global/globalThis surface.
import { loadCount } from "./00_api";

const count: Promise<number> = loadCount();
const wrongCount: Promise<string> = loadCount(); // error[TK2322]
