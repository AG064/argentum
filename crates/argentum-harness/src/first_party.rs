use argentum_domain::HarnessCapabilityKind;

use crate::{available_capability, lifecycle::HarnessModuleRegistration, CapabilityRegistration};

pub const FIRST_PARTY_REVIEW_DIFF_MODULE_ID: &str = "first-party.review-diff";
pub const REVIEW_DIFF_CAPABILITY_ID: &str = "review.diff";

pub fn first_party_review_diff_module() -> HarnessModuleRegistration {
    HarnessModuleRegistration {
        id: FIRST_PARTY_REVIEW_DIFF_MODULE_ID.to_owned(),
        dependencies: Vec::new(),
        capabilities: vec![review_diff_capability()],
    }
}

pub(crate) fn review_diff_capability() -> CapabilityRegistration {
    available_capability(
        REVIEW_DIFF_CAPABILITY_ID,
        "Per-file diff review",
        HarnessCapabilityKind::Review,
        "Bounded explicit unified diffs and approved chronological restore are available.",
        vec!["changes.accounting", "approval.write"],
    )
}
