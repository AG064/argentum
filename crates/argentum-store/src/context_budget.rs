use argentum_domain::context_budget::ModelBudget;
use argentum_domain::{ProjectId, ProviderProfile};
use rusqlite::{params, OptionalExtension};

use super::{load_provider_profiles, Store, StoreError};

impl Store {
    /// Trusted configuration API. The command host must keep this off JSONL and
    /// refuse mutation while a task owns the project. No provider metadata writes.
    pub fn save_model_budget(
        &self,
        project_id: ProjectId,
        budget: &ModelBudget,
    ) -> Result<(), StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction()?;
        let profile = load_provider_profiles(&tx, project_id)?
            .into_iter()
            .find(|profile| profile.id == budget.profile_id)
            .ok_or_else(|| StoreError::InvalidRecord("budget profile is unavailable".into()))?;
        budget
            .validate(&profile)
            .map_err(|reason| StoreError::InvalidRecord(reason.into()))?;
        let payload = serde_json::to_string(budget)?;
        if payload.len() > 8192 {
            return Err(StoreError::InvalidRecord(
                "budget record exceeds limit".into(),
            ));
        }
        tx.execute(
            "INSERT INTO model_budgets(project_id, profile_id, payload) VALUES(?1, ?2, ?3)
            ON CONFLICT(project_id, profile_id) DO UPDATE SET payload=excluded.payload",
            params![project_id.to_string(), budget.profile_id, payload],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn model_budget(
        &self,
        project_id: ProjectId,
        profile: &ProviderProfile,
    ) -> Result<Option<ModelBudget>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        // Check size in SQL before allocating a possibly tampered body.
        let payload: Option<Option<String>> = connection
            .query_row(
                "SELECT CASE WHEN length(CAST(payload AS BLOB)) <= 8192 THEN payload ELSE NULL END
             FROM model_budgets WHERE project_id=?1 AND profile_id=?2",
                params![project_id.to_string(), profile.id],
                |row| row.get(0),
            )
            .optional()?;
        match payload {
            None => Ok(None),
            Some(None) => Err(StoreError::InvalidRecord(
                "budget record exceeds limit".into(),
            )),
            Some(Some(payload)) => {
                let budget: ModelBudget = serde_json::from_str(&payload)
                    .map_err(|_| StoreError::InvalidRecord("malformed model budget".into()))?;
                budget
                    .validate(profile)
                    .map_err(|reason| StoreError::InvalidRecord(reason.into()))?;
                Ok(Some(budget))
            }
        }
    }
}
