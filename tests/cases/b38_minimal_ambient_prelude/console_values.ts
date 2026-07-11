// Backlog 38 — the prelude provides only the basic console methods needed for
// early signal. Cross-checked with tsc 6.0.3 --strict.

console.log("ready", 1, { enabled: true });
console.warn("careful");
console.error({ message: "failed" });

const wrongConsoleResult: string = console.log("ready"); // error[TK2322]: Type 'void' is not assignable to type 'string'
console.missing("missing"); // error[TK2339]: Property 'missing' does not exist
