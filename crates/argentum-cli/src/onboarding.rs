pub use argentum_domain::{
    GuidanceLevel, OnboardingAction as SetupAction, OnboardingConnectionState,
    OnboardingCredentialState, OnboardingError, OnboardingSnapshot, OnboardingStep,
};

#[derive(Debug, Clone)]
pub struct SetupState {
    snapshot: OnboardingSnapshot,
}

impl SetupState {
    pub fn new(snapshot: OnboardingSnapshot) -> Self {
        let normalized = snapshot.clone().normalized().unwrap_or(snapshot);
        Self {
            snapshot: normalized,
        }
    }

    pub fn snapshot(&self) -> &OnboardingSnapshot {
        &self.snapshot
    }

    pub fn step(&self) -> OnboardingStep {
        self.snapshot.step
    }

    pub fn apply(&mut self, action: SetupAction) -> Result<(), OnboardingError> {
        self.snapshot.apply(action)
    }
}
