//! Output records shared by the separate reporting authorities.

use crate::diagnostics::{Diagnostic, IncompleteSurface};

#[derive(Clone, Debug)]
pub(crate) enum CheckerRecord {
    Diagnostic(Diagnostic),
    Incomplete(IncompleteSurface),
}
