//! Project-owned default-library semantic candidate boundary.

mod cache;
mod contract;
mod coordinator;
mod entry;
#[cfg(any(test, feature = "test-utils"))]
mod fault_gate;
#[cfg(any(test, feature = "test-utils"))]
mod global_owner;
mod lowerer;
#[cfg(any(test, feature = "test-utils"))]
mod observation;
#[cfg(any(test, feature = "test-utils"))]
pub mod project_local_pass_port;
#[cfg(all(feature = "session-bench", not(any(test, feature = "test-utils"))))]
mod project_local_pass_port;
mod recipe_primitives;
mod record_staging;
mod session;
mod terminal;
pub(super) mod validation;

#[cfg(any(test, feature = "test-utils"))]
pub use contract::*;
#[cfg(all(feature = "session-bench", not(any(test, feature = "test-utils"))))]
pub(super) use contract::*;
#[cfg(any(test, feature = "test-utils"))]
pub use entry::*;
#[cfg(test)]
pub(super) use lowerer::{
    accumulate_product_library_records_for_test, lower_product_groups_for_test,
    ProductLowererConstructionScopeForTest,
};
#[cfg(any(test, feature = "test-utils"))]
pub use observation::{
    observation_storage_counts_for_test, ObservationEventForTest, ObservationEventKindForTest,
    ObservationReceiptForTest, ObservationScopeForTest, ObservationWitnessKindForTest,
    ProjectLocalFaultSeamForTest, ProjectLocalSemanticPhaseForTest, WeakLivenessForTest,
};
pub use session::{CheckerSession, CheckerSessionError};
#[cfg(feature = "session-bench")]
pub use session::{
    SessionBenchCheckReceipt, SessionBenchClock, SessionBenchFault, SessionBenchFaultReceipt,
    SessionBenchFaultSeam,
};
