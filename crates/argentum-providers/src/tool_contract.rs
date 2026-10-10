use std::collections::BTreeSet;

use super::{invalid_tool_request, valid_tool_name, ModelToolDefinition, ProviderError};

/// Shared local catalog budget for runtime assembly and HTTP request validation.
/// This is not a claim about a vendor's API limit or a grant of tool authority.
/// Adapters must still enforce any narrower capability restrictions they support.
pub struct ModelToolContract;

impl ModelToolContract {
    /// Bounded headroom above the complete 17-tool built-in catalog.
    pub const MAX_DEFINITIONS: usize = 32;
    /// Existing serialized parameter-schema byte limit, unchanged by F01.
    pub const MAX_SCHEMA_BYTES: usize = 32 * 1024;

    pub fn validate_catalog(tools: &[ModelToolDefinition]) -> Result<(), ProviderError> {
        if tools.len() > Self::MAX_DEFINITIONS {
            return Err(invalid_tool_request("too many tool definitions"));
        }
        let mut names = BTreeSet::new();
        for tool in tools {
            if !valid_tool_name(&tool.name) || !names.insert(tool.name.as_str()) {
                return Err(invalid_tool_request("invalid tool definition identity"));
            }
            if !tool.parameters.is_object()
                || serde_json::to_vec(&tool.parameters)
                    .map_err(|_| invalid_tool_request("invalid tool definition schema"))?
                    .len()
                    > Self::MAX_SCHEMA_BYTES
            {
                return Err(invalid_tool_request("invalid tool definition schema"));
            }
        }
        Ok(())
    }
}
