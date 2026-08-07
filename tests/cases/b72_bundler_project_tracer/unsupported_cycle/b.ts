import { fromA } from "./a.js";

export function fromB(): number {
  return typeof fromA === "function" ? 1 : 0;
}
