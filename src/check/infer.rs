//! Inference entry points (architecture §5.1, mvp-plan §3).
//!
//! Inference is *generative*, not relational — it produces types from
//! constraints (contextual typing, `infer` extraction, type-argument inference)
//! and is roughly as large as the relation engine. It gets its own module from
//! day 1 so it is never silently folded into the checker or the relater.
//!
//! Through M1 the inference needed is still trivial — the type of a literal
//! expression and `const`/`let` initializer inference with `let` widening
//! (literal → base) — and is implemented in `checker.rs` directly (a few lines
//! per case). The real candidate-collection + constraint-solving machine lands
//! later.
//!
//! TODO(post-MVP): contextual typing and generic type-argument inference attach
//! here as a standalone engine, never folded into the checker or the relater.

// Intentionally empty beyond this documentation. No reachable code path here —
// the foundation module marks where the inference engine attaches.
