//! Project-local semantic publication terminals.

#[cfg(any(test, feature = "test-utils"))]
use super::observation;
#[cfg(any(test, feature = "test-utils"))]
use super::observation::ProjectLocalSemanticPhaseForTest;
use super::project_local_pass_port::TypeId;
use super::ProjectLocalUnavailableReasonForTest;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ProductTerminal {
    Resolving,
    Ready {
        identity: String,
        ty: Option<TypeId>,
        value_ty: Option<TypeId>,
    },
    Unavailable(ProjectLocalUnavailableReasonForTest),
}

impl ProductTerminal {
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn identity(&self) -> String {
        match self {
            Self::Resolving => "resolving".to_owned(),
            Self::Ready { identity, .. } => identity.clone(),
            Self::Unavailable(reason) => {
                let mut identity = "unavailable:".to_owned();
                identity.push_str(match reason {
                    ProjectLocalUnavailableReasonForTest::UnsupportedProductRecipe => {
                        "unsupported-product-recipe"
                    }
                    ProjectLocalUnavailableReasonForTest::UnsupportedUserDeclaration => {
                        "unsupported-user-declaration"
                    }
                    ProjectLocalUnavailableReasonForTest::InvalidUserDeclaration => {
                        "invalid-user-declaration"
                    }
                    ProjectLocalUnavailableReasonForTest::CyclicUserDeclaration => {
                        "cyclic-user-declaration"
                    }
                });
                identity
            }
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn materialize(phase: ProjectLocalSemanticPhaseForTest, product_group: usize) {
        observation::record_owner_materialized(phase, product_group);
    }
}
