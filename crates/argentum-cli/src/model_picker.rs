use argentum_domain::ProviderModel;
use thiserror::Error;

pub const MAX_VISIBLE_MODELS: usize = 64;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelPickerError {
    #[error("model selection must not be empty")]
    Empty,
    #[error("model selection index must be a positive number")]
    InvalidIndex,
    #[error("model selection index is outside the displayed catalog")]
    IndexOutOfRange,
    #[error("model is not present in the provider catalog")]
    UnknownModel,
    #[error("manual model entry is unavailable while a catalog is active")]
    ManualNotAllowed,
    #[error("model must be at most 256 characters and contain no control characters")]
    InvalidManualModel,
}

pub fn bounded_models(models: &[ProviderModel]) -> Vec<ProviderModel> {
    models.iter().take(MAX_VISIBLE_MODELS).cloned().collect()
}

pub fn filter_models(models: &[ProviderModel], query: &str) -> Vec<ProviderModel> {
    let query = query.trim().to_ascii_lowercase();
    bounded_models(models)
        .into_iter()
        .filter(|model| {
            query.is_empty()
                || model.id.to_ascii_lowercase().contains(&query)
                || model.label.to_ascii_lowercase().contains(&query)
        })
        .collect()
}

pub fn select_model(
    models: &[ProviderModel],
    input: &str,
    allow_manual: bool,
) -> Result<String, ModelPickerError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ModelPickerError::Empty);
    }
    if let Ok(index) = input.parse::<usize>() {
        if index == 0 {
            return Err(ModelPickerError::InvalidIndex);
        }
        return bounded_models(models)
            .get(index - 1)
            .map(|model| model.id.clone())
            .ok_or(ModelPickerError::IndexOutOfRange);
    }
    if let Some(model) = bounded_models(models)
        .into_iter()
        .find(|model| model.id == input)
    {
        return Ok(model.id);
    }
    if !allow_manual {
        return Err(if models.is_empty() {
            ModelPickerError::UnknownModel
        } else {
            ModelPickerError::ManualNotAllowed
        });
    }
    if input.chars().count() > 256 || input.chars().any(char::is_control) {
        return Err(ModelPickerError::InvalidManualModel);
    }
    Ok(input.to_owned())
}
