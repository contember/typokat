//! Output records shared by the separate reporting authorities.

use crate::diagnostics::{Diagnostic, IncompleteSurface};

#[derive(Clone, Debug)]
pub(crate) enum CheckerRecord {
    Diagnostic(Diagnostic),
    Incomplete(IncompleteSurface),
}

impl CheckerRecord {
    pub(crate) const fn is_diagnostic(&self) -> bool {
        matches!(self, Self::Diagnostic(_))
    }
}
