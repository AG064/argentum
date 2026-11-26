// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Model per task: picks the model for each step of a run from the owner's
//! tiers, and says in run details which model did what.
//!
//! Rules, in order:
//!
//! - A step with no tier uses the selected model.
//! - A tier on another provider profile is not used inside a run: the step
//!   uses the selected model, so the conversation never goes to another
//!   provider because of a tier.
//! - A tier whose model has a known context window smaller than the selected
//!   model's is not used, since the run's budget was set for the selected
//!   model.
//! - Spend limits and goal budgets are those of the run's provider profile,
//!   which a tier never changes. Neither does it change permissions,
//!   approvals or the policy: a tier only names the model in the request.

use argentum_domain::context_budget::ModelBudget;
use argentum_domain::model_tiers::{ModelStep, ModelTierSettings, StepModelChoice};
use argentum_domain::{AppEvent, ProviderProfile, RunId, SessionId};

use super::{RuntimeError, RuntimeService};

/// Picks a step's model. `window` gives a model's known context window on
/// the run's profile.
pub(super) fn choose(
    settings: &ModelTierSettings,
    step: ModelStep,
    profile: &ProviderProfile,
    window: impl Fn(&str) -> Option<u64>,
) -> StepModelChoice {
    let selected = |reason: String| StepModelChoice {
        step,
        model: profile.model.clone(),
        tier: None,
        reason,
    };
    let Some(tier) = settings.tier_for(step) else {
        return selected("no tier is set for this step, so the selected model was used".into());
    };
    if tier.profile_id != profile.id {
        return selected(format!(
            "tier {} is on another provider profile, and a step never sends the conversation to another provider, so the selected model was used",
            tier.name
        ));
    }
    if tier.model != profile.model {
        if let (Some(tier_window), Some(selected_window)) =
            (window(&tier.model), window(&profile.model))
        {
            if tier_window < selected_window {
                return selected(format!(
                    "tier {} has a smaller context window than the selected model, so the selected model was used",
                    tier.name
                ));
            }
        }
    }
    StepModelChoice {
        step,
        model: tier.model.clone(),
        tier: Some(tier.name.clone()),
        reason: format!("tier {}", tier.name),
    }
}

impl RuntimeService {
    /// The model for one step of a run on `profile`.
    pub(super) fn step_model(
        &self,
        step: ModelStep,
        profile: &ProviderProfile,
    ) -> Result<StepModelChoice, RuntimeError> {
        let settings = self.store.model_tiers(self.project_id)?;
        if settings.tier_for(step).is_none() {
            return Ok(choose(&settings, step, profile, |_| None));
        }
        Ok(choose(&settings, step, profile, |model| {
            self.profile_catalog_context_limit(profile, model)
                .ok()
                .flatten()
        }))
    }

    /// The profile and budget a step's request is prepared with. A tier's
    /// model gets a copy of the run's profile and budget with only the model
    /// changed, and only when that model's known context window holds the
    /// run's limits; the provider checks that the request names the model its
    /// budget was made for. Otherwise the step uses the selected model.
    pub(super) fn step_route(
        &self,
        step: ModelStep,
        profile: &ProviderProfile,
        budget: &ModelBudget,
    ) -> Result<(ProviderProfile, ModelBudget, StepModelChoice), RuntimeError> {
        let choice = self.step_model(step, profile)?;
        if choice.model == profile.model {
            return Ok((profile.clone(), budget.clone(), choice));
        }
        let selected = |reason: String| {
            (
                profile.clone(),
                budget.clone(),
                StepModelChoice {
                    step,
                    model: profile.model.clone(),
                    tier: None,
                    reason,
                },
            )
        };
        let tier = choice.tier.clone().unwrap_or_default();
        let mut step_profile = profile.clone();
        step_profile.model = choice.model.clone();
        let window = argentum_providers::bundled_context_window(&step_profile).or(self
            .profile_catalog_context_limit(profile, &choice.model)
            .ok()
            .flatten());
        if !window.is_some_and(|window| window >= budget.usable_context_tokens) {
            return Ok(selected(format!(
                "tier {tier}: its model has no known context window as large as the selected model's limits, so the selected model was used"
            )));
        }
        let mut step_budget = budget.clone();
        step_budget.model = choice.model.clone();
        if self
            .validate_model_budget(&step_profile, &step_budget)
            .is_err()
        {
            return Ok(selected(format!(
                "tier {tier}: the run's limits do not fit its model, so the selected model was used"
            )));
        }
        Ok((step_profile, step_budget, choice))
    }

    /// Journals a step's choice so run details can show it. Only a step the
    /// owner set a tier for is journaled: without tiers a run and a compaction
    /// write exactly the events they wrote before tiers existed.
    pub(super) fn record_step_model(
        &self,
        session_id: SessionId,
        run_id: RunId,
        choice: StepModelChoice,
    ) -> Result<(), RuntimeError> {
        if self
            .store
            .model_tiers(self.project_id)?
            .tier_for(choice.step)
            .is_none()
        {
            return Ok(());
        }
        self.emit_for_run(
            AppEvent::ModelStepChosen {
                session_id,
                run_id,
                choice,
            },
            run_id,
        )?;
        Ok(())
    }

    pub(super) fn publish_model_tiers(
        &self,
        notice: Option<(String, bool)>,
    ) -> Result<(), RuntimeError> {
        let settings = self.store.model_tiers(self.project_id)?;
        let stored = self.store.provider_profiles(self.project_id)?;
        let selected_profile_id = stored
            .iter()
            .find(|profile| profile.selected)
            .map(|profile| profile.id.clone());
        let profiles = stored
            .into_iter()
            .map(|profile| (profile.id, profile.label))
            .collect();
        let (notice, refused) =
            notice.map_or((None, false), |(text, refused)| (Some(text), refused));
        self.publish_transient(AppEvent::ModelTiersLoaded {
            settings,
            profiles,
            selected_profile_id,
            notice,
            refused,
        });
        Ok(())
    }

    pub(super) fn save_model_tiers(&self, settings: ModelTierSettings) -> Result<(), RuntimeError> {
        let profiles = self.store.provider_profiles(self.project_id)?;
        let notice = if let Some(tier) = settings
            .tiers
            .iter()
            .find(|tier| !profiles.iter().any(|profile| profile.id == tier.profile_id))
        {
            (
                format!(
                    "tier {} names a provider profile that is not set up; set it up first",
                    tier.name
                ),
                true,
            )
        } else {
            match self.store.save_model_tiers(self.project_id, &settings) {
                Ok(()) => ("Model tiers saved.".to_owned(), false),
                Err(argentum_store::StoreError::InvalidRecord(reason)) => (reason, true),
                Err(error) => return Err(error.into()),
            }
        };
        self.publish_model_tiers(Some(notice))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use argentum_domain::model_tiers::ModelTier;

    use super::*;

    fn profile() -> ProviderProfile {
        ProviderProfile {
            id: "deepseek".into(),
            label: "DeepSeek".into(),
            kind: argentum_domain::ProviderKind::OpenAiCompatible,
            endpoint: "https://api.deepseek.com".into(),
            model: "deepseek-reasoner".into(),
            selected: true,
        }
    }

    fn settings(profile_id: &str, model: &str) -> ModelTierSettings {
        ModelTierSettings {
            tiers: vec![ModelTier {
                name: "cheap".into(),
                profile_id: profile_id.into(),
                model: model.into(),
            }],
            steps: BTreeMap::from([(ModelStep::Summaries, "cheap".into())]),
        }
    }

    #[test]
    fn a_step_without_a_tier_uses_the_selected_model() {
        let choice = choose(
            &settings("deepseek", "deepseek-chat"),
            ModelStep::Planning,
            &profile(),
            |_| None,
        );
        assert_eq!(choice.model, "deepseek-reasoner");
        assert_eq!(choice.tier, None);
    }

    #[test]
    fn a_tier_on_the_same_profile_picks_its_model() {
        let choice = choose(
            &settings("deepseek", "deepseek-chat"),
            ModelStep::Summaries,
            &profile(),
            |_| Some(128_000),
        );
        assert_eq!(choice.model, "deepseek-chat");
        assert_eq!(choice.tier.as_deref(), Some("cheap"));
    }

    #[test]
    fn a_tier_on_another_provider_is_never_used_inside_a_run() {
        let choice = choose(
            &settings("local", "qwen"),
            ModelStep::Summaries,
            &profile(),
            |_| Some(1_000_000),
        );
        assert_eq!(choice.model, "deepseek-reasoner");
        assert!(choice.reason.contains("another provider"));
    }

    #[test]
    fn a_smaller_context_window_keeps_the_selected_model() {
        let choice = choose(
            &settings("deepseek", "small"),
            ModelStep::Summaries,
            &profile(),
            |model| Some(if model == "small" { 8_000 } else { 128_000 }),
        );
        assert_eq!(choice.model, "deepseek-reasoner");
        assert!(choice.reason.contains("smaller context window"));
    }
}
