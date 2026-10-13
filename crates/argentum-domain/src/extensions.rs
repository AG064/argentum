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
    /// Optional Windows application adapter. Exact activation is independent
    /// of a browser or other reviewed-code connection.
    ReviewedWindowsApplication,
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
        #[serde(default)]
        cursor: Option<String>,
    },
    ReadResource {
        namespace: String,
        handle: Value,
    },
}

/// Closed, body-free metadata. These receipts are observations, never grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionOperationKind {
    ConnectionTeardown,
    List,
    Skill,
    Catalog,
    Invoke,
    ResourceCatalog,
    ResourceRead,
    BrowserPreview,
    BrowserOpen,
    BrowserObserve,
    BrowserInteract,
    BrowserViewport,
    BrowserScreenshot,
    BrowserDiagnostics,
    BrowserClose,
    DesktopLaunch,
    DesktopObserve,
    DesktopAction,
    DesktopScreenshot,
    DesktopClose,
}

impl ExtensionOperation {
    pub fn receipt_kind(&self) -> ExtensionOperationKind {
        use ExtensionOperationKind as K;
        match self {
            Self::List => K::List,
            Self::LoadSkill { .. } => K::Skill,
            Self::DiscoverTools { .. } => K::Catalog,
            Self::DiscoverResources { .. } => K::ResourceCatalog,
            Self::ReadResource { .. } => K::ResourceRead,
            Self::Invoke { handle, .. } => match handle.get("name").and_then(Value::as_str) {
                Some("browser_preview_start") => K::BrowserPreview,
                Some("browser_open") => K::BrowserOpen,
                Some("browser_observe") => K::BrowserObserve,
                Some("browser_interact") => K::BrowserInteract,
                Some("browser_viewport") => K::BrowserViewport,
                Some("browser_screenshot") => K::BrowserScreenshot,
                Some("browser_diagnostics") => K::BrowserDiagnostics,
                Some("browser_close") => K::BrowserClose,
                Some("desktop_launch") => K::DesktopLaunch,
                Some("desktop_observe") => K::DesktopObserve,
                Some("desktop_action") => K::DesktopAction,
                Some("desktop_screenshot") => K::DesktopScreenshot,
                Some("desktop_close") => K::DesktopClose,
                _ => K::Invoke,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionReceiptStage {
    Requested,
    Admitted,
    Completed,
    Teardown,
    ImagePreparedForDispatch,
    ImageAcceptedByTransport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionReceiptOutcome {
    Pending,
    Returned,
    Rejected,
    ServerFailed,
    Cancelled,
    Unavailable,
    Drained,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionTransportObservation {
    pub mcp_request_starts: u64,
    pub mcp_notification_starts: u64,
    pub mcp_response_starts: u64,
    pub tool_call_starts: u64,
    pub completed_messages: u64,
    pub repeated_request_ids: u64,
    pub sent_bytes: u64,
    pub received_bytes: u64,
    pub http_request_starts: Option<u64>,
    pub coverage_incomplete: bool,
}

/// Server claims are kept separate from host transport and image validation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionBrowserReport {
    pub browser_id: Option<Uuid>,
    pub context_id: Option<Uuid>,
    pub page_id: Option<Uuid>,
    pub observation_generation: Option<u64>,
    pub actions: Option<u64>,
    pub viewport_width: Option<u32>,
    pub viewport_height: Option<u32>,
    pub closed: Option<bool>,
    pub screenshot_sha256: Option<Sha256Digest>,
    pub screenshot_bytes: Option<u64>,
    pub failure: Option<BrowserFailureReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserFailureReason {
    StaleObservation,
    StaleTarget,
    DestinationDenied,
    SelectionInvalid,
    NavigationFailed,
    Capacity,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionImageObservation {
    pub sha256: Sha256Digest,
    pub encoded_bytes: u64,
    pub context_id: Option<Uuid>,
    pub request_id: Uuid,
}

/// Producer-reported native observations, never host verification or authority.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionDesktopReport {
    pub context_id: Option<Uuid>,
    pub observation_generation: Option<u64>,
    pub process_id: Option<u32>,
    pub process_creation: Option<u64>,
    pub primitive_actions: Option<u64>,
    /// Producer-reported API returns, not observed application outcomes.
    #[serde(default)]
    pub api_returned_actions: Option<u64>,
    pub api_returned: Option<bool>,
    pub closed: Option<bool>,
    pub failure: Option<DesktopFailureReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopFailureReason {
    StaleObservation,
    OwnerTakeover,
    Capacity,
    UncertainEffect,
    Unavailable,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionOperationReceipt {
    pub version: u8,
    pub call_id: Uuid,
    pub kind: ExtensionOperationKind,
    pub stage: ExtensionReceiptStage,
    pub outcome: ExtensionReceiptOutcome,
    pub connection_id: Option<Uuid>,
    pub connection_generation: Option<Uuid>,
    pub definition_revision: Option<Uuid>,
    /// None is unavailable, not proof that nothing was dispatched.
    pub tool_dispatch_observed: Option<bool>,
    pub transport: Option<ExtensionTransportObservation>,
    pub server_report: Option<ExtensionBrowserReport>,
    #[serde(default)]
    pub desktop_report: Option<ExtensionDesktopReport>,
    pub image: Option<ExtensionImageObservation>,
    pub application_condition_verified: bool,
}

impl ExtensionOperationReceipt {
    pub fn requested(call_id: Uuid, kind: ExtensionOperationKind) -> Self {
        Self {
            version: 1,
            call_id,
            kind,
            stage: ExtensionReceiptStage::Requested,
            outcome: ExtensionReceiptOutcome::Pending,
            connection_id: None,
            connection_generation: None,
            definition_revision: None,
            tool_dispatch_observed: None,
            transport: None,
            server_report: None,
            desktop_report: None,
            image: None,
            application_condition_verified: false,
        }
    }
}
