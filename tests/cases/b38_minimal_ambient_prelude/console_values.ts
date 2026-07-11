// Backlog 38 — the prelude provides only the basic console methods needed for
// early signal. Cross-checked with tsc 6.0.3 --strict.

console.log("ready", 1, { enabled: true });
console.warn("careful");
console.error({ message: "failed" });
