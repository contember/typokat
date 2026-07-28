//! Output records shared by the separate reporting authorities.

use crate::diagnostics::{Diagnostic, IncompleteSurface};

#[derive(Clone, Debug)]
pub enum CheckerRecord {
    Diagnostic(Diagnostic),
    Incomplete(IncompleteSurface),
}

impl CheckerRecord {
    #[cfg(any(test, feature = "test-utils"))]
    pub const fn is_diagnostic(&self) -> bool {
        matches!(self, Self::Diagnostic(_))
    }
}
