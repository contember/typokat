//! Project-owned default-library semantic candidate boundary.

mod cache;
mod contract;
mod coordinator;
mod entry;
mod fault_gate;
mod global_owner;
mod lowerer;
mod observation;
pub mod project_local_pass_port;
mod recipe_primitives;
mod record_staging;
mod session;
mod terminal;
pub(super) mod validation;

pub use contract::*;
pub use entry::*;
#[cfg(test)]
pub(super) use lowerer::{
    accumulate_product_library_records_for_test, lower_product_groups_for_test,
};
pub use observation::{
    observation_storage_counts_for_test, ObservationEventForTest, ObservationEventKindForTest,
    ObservationReceiptForTest, ObservationScopeForTest, ObservationWitnessKindForTest,
    ProjectLocalFaultSeamForTest, ProjectLocalSemanticPhaseForTest, WeakLivenessForTest,
};
pub use session::{CheckerSession, CheckerSessionError};
