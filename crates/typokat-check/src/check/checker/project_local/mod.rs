//! Project-owned default-library semantic candidate boundary.

mod contract;
mod entry;
mod fault_gate;
mod observation;
pub mod project_local_pass_port;

pub use contract::*;
pub use entry::*;
pub use observation::{
    ObservationEventForTest, ObservationEventKindForTest, ObservationReceiptForTest,
    ObservationScopeForTest, ObservationWitnessKindForTest, ProjectLocalFaultSeamForTest,
    ProjectLocalSemanticPhaseForTest, WeakLivenessForTest,
};
