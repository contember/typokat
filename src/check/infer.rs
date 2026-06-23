//! Inference entry points (architecture §5.1, mvp-plan §3).
//!
//! Inference is *generative*, not relational — it produces types from
//! constraints (contextual typing, `infer` extraction, type-argument inference)
//! and is roughly as large as the relation engine. It gets its own module from
//! day 1 so it is never silently folded into the checker or the relater.
//!
//! M0 needs only the trivial case — the type of a literal expression — which is
//! implemented in `checker.rs` directly (a one-liner per literal kind). The real
//! candidate-collection + constraint-solving machine lands later.
//!
//! TODO(M1: inference): infer `const`/`let` types from initializers, with `let`
//! widening (literal → base) and `const` keeping the literal type.
//! TODO(post-MVP): contextual typing and generic type-argument inference.

// Intentionally empty for M0 beyond this documentation. No reachable code path
// here — the foundation module marks where the inference engine attaches.
