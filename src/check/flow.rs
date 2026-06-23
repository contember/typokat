//! Flow analysis & narrowing stubs (architecture §5, mvp-plan §3/§7.3).
//!
//! Control-flow narrowing (`typeof`, `in`, truthiness, equality, discriminated
//! unions, assertion functions) is *the* reason people love TS DX and must never
//! be sacrificed. It is flow-sensitive and needs a native interpreter model, so
//! the checker is structured around flow nodes from the start even though the MVP
//! implements no narrowing logic yet (mvp-plan §7.3).
//!
//! TODO(M7: flow/narrowing): build a flow graph (flow nodes for assignments,
//! branches, loops) and a narrowing pass that refines a declared type along a
//! given flow path. Definite-assignment (`TK2454`) and reachability (`TK2355`)
//! also live here.

/// A node in the control-flow graph. Reserved shape for the narrowing milestone;
/// not constructed in M0.
#[allow(dead_code)] // TODO(M7): produced by the binder/checker flow pass.
pub enum FlowNode {
    /// The unreachable start sentinel.
    Unreachable,
    /// Flow start (function/module entry).
    Start,
    // TODO(M7): Assignment, Condition (true/false branch), Loop, Join, Call, …
}
