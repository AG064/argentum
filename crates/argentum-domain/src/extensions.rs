//! Serializable declarations and non-secret approvals, never live authority.
use crate::Sha256Digest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionDefinition {
    pub namespace: String,
    pub package_root: String,
    pub package_sha256: Sha256Digest,
    pub source_id: String,
    pub source_revision: String,
    pub transport: ExtensionTransport,
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub resource_prefixes: Vec<String>,
    /// Local executable code runs with the current user's OS access. Job limits
    /// are resource/lifetime containment, not filesystem/network sandboxing.
    pub trust: ExtensionTrust,
    #[serde(default)]
    pub image_scope: Option<ExtensionImageScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionImageScope {
    pub directory: String,
    pub origins: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionTrust {
    ReadOnlyInstructions,
    ReviewedCurrentUserCode,
    ReviewedRemoteService,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionTransport {
    Skills,
    Stdio {
        program: String,
        program_sha256: Sha256Digest,
        args: Vec<String>,
        cwd: String,
        #[serde(default)]
        environment: BTreeMap<String, String>,
        #[serde(default)]
        inputs: BTreeMap<String, Sha256Digest>,
    },
    StreamableHttp {
        endpoint: String,
        addresses: Vec<String>,
        #[serde(default)]
        local_demo: bool,
        #[serde(default)]
        credential_account: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionRecord {
    pub definition: ExtensionDefinition,
    pub revision: Uuid,
    pub active: bool,
    pub approval_binding: Option<Sha256Digest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionOperation {
    List,
    LoadSkill {
        namespace: String,
        path: String,
    },
    DiscoverTools {
        namespace: String,
        #[serde(default)]
        cursor: Option<String>,
    },
    Invoke {
        namespace: String,
        handle: Value,
        arguments: Value,
    },
    DiscoverResources {
        namespace: String,
    },
    ReadResource {
        namespace: String,
        handle: Value,
    },
}
