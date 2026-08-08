import { renamed, RenamedShape } from "./barrel.js";
const goodValue: number = renamed;
const goodShape: RenamedShape = { width: 1 };
const badValue: string = renamed;
const badTypeUse = RenamedShape;
