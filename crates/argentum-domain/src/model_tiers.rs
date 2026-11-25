// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Model per task: tiers the owner names per project, such as `cheap`,
//! `strong` or `local`, and which tier each step of a run uses.
//!
//! A tier only picks a model. Switching model never changes permissions,
//! approvals or the policy. A run uses a tier only when it is on the run's
//! own provider profile, so a step never sends the conversation to another
//! provider; a tier on another profile is kept but not used inside a run.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const MAX_TIERS: usize = 8;
pub const MAX_TIER_NAME_BYTES: usize = 32;
pub const MAX_TIER_MODEL_BYTES: usize = 200;

/// The steps of a run a tier can be set for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStep {
    /// The first request of a run, before any tool result.
    Planning,
    /// Every later request of a run, where the agent reads and edits.
    Editing,
    /// Compaction summaries.
    Summaries,
}

impl ModelStep {
    pub const ALL: [Self; 3] = [Self::Planning, Self::Editing, Self::Summaries];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Editing => "editing",
            Self::Summaries => "summaries",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|step| step.as_str() == value)
    }
}

/// A named model on one of the owner's provider profiles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTier {
    pub name: String,
    pub profile_id: String,
    pub model: String,
}

/// A project's tiers and the tier each step uses. A step with no tier uses
/// the selected model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTierSettings {
    #[serde(default)]
    pub tiers: Vec<ModelTier>,
    #[serde(default)]
    pub steps: BTreeMap<ModelStep, String>,
}

pub fn valid_tier_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_TIER_NAME_BYTES
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn valid_model(model: &str) -> bool {
    !model.trim().is_empty()
        && model.len() <= MAX_TIER_MODEL_BYTES
        && !model.chars().any(|c| c.is_control() || c.is_whitespace())
}

impl ModelTierSettings {
    pub fn tier(&self, name: &str) -> Option<&ModelTier> {
        self.tiers.iter().find(|tier| tier.name == name)
    }

    /// The tier set for a step, if any.
    pub fn tier_for(&self, step: ModelStep) -> Option<&ModelTier> {
        self.steps.get(&step).and_then(|name| self.tier(name))
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.tiers.len() > MAX_TIERS {
            return Err("at most 8 tiers");
        }
        let mut names = std::collections::BTreeSet::new();
        for tier in &self.tiers {
            if !valid_tier_name(&tier.name) {
                return Err("a tier name is 1 to 32 lower-case letters, digits, - or _");
            }
            if !names.insert(tier.name.as_str()) {
                return Err("two tiers have the same name");
            }
            if tier.profile_id.is_empty() || tier.profile_id.len() > 64 {
                return Err("a tier needs a provider profile");
            }
            if !valid_model(&tier.model) {
                return Err("a tier needs a model name without spaces");
            }
        }
        if self.steps.values().any(|name| self.tier(name).is_none()) {
            return Err("a step uses a tier that does not exist");
        }
        Ok(())
    }
}

/// The model a step used, and why, for run details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepModelChoice {
    pub step: ModelStep,
    pub model: String,
    /// The tier that picked the model, or `None` for the selected model.
    pub tier: Option<String>,
    /// Why, in plain words, such as "tier cheap" or "tier local is on another
    /// provider, so the selected model was used".
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(name: &str, model: &str) -> ModelTier {
        ModelTier {
            name: name.into(),
            profile_id: "deepseek".into(),
            model: model.into(),
        }
    }

    #[test]
    fn settings_are_checked() {
        let mut settings = ModelTierSettings {
            tiers: vec![
                tier("cheap", "deepseek-chat"),
                tier("strong", "deepseek-reasoner"),
            ],
            steps: BTreeMap::from([
                (ModelStep::Planning, "strong".into()),
                (ModelStep::Summaries, "cheap".into()),
            ]),
        };
        assert!(settings.validate().is_ok());
        assert_eq!(
            settings.tier_for(ModelStep::Planning).unwrap().model,
            "deepseek-reasoner"
        );
        assert!(settings.tier_for(ModelStep::Editing).is_none());
        settings.steps.insert(ModelStep::Editing, "missing".into());
        assert!(settings.validate().is_err());
        settings.steps.remove(&ModelStep::Editing);
        settings.tiers.push(tier("cheap", "x"));
        assert!(settings.validate().is_err());
        settings.tiers.pop();
        settings.tiers.push(tier("Bad Name", "x"));
        assert!(settings.validate().is_err());
    }

    #[test]
    fn steps_round_trip_as_json_keys() {
        let settings = ModelTierSettings {
            tiers: vec![tier("cheap", "m")],
            steps: BTreeMap::from([(ModelStep::Summaries, "cheap".into())]),
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"summaries\":\"cheap\""));
        assert_eq!(
            serde_json::from_str::<ModelTierSettings>(&json).unwrap(),
            settings
        );
    }
}
