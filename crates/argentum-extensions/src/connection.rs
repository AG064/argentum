//! MCP sessions have reviewed connection identity, fresh catalog handles and
//! bounded results. Server instructions and annotations are never host policy.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use argentum_execution::{ManagedIpcProcess, ManagedProcessLimits, ProcessSpec};
use rmcp::model::{
    CallToolRequestParams, ClientCapabilities, ClientConfig, ClientRequest, Implementation,
    ListResourcesRequest, ListToolsRequest, PaginatedRequestParams, ProtocolVersion,
    ReadResourceRequest, ReadResourceRequestParams, ServerResult,
};
use rmcp::service::RunningService;
use rmcp::{ClientLifecycleMode, ClientServiceExt, RoleClient};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    wire::{BoundedStdio, WireAccounting},
    MAX_CATALOG_ENTRIES, MAX_RESULT_BYTES,
};

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("extension process failure: {0}")]
    Process(#[from] argentum_execution::ProcessError),
    #[error("extension connection rejected: {0}")]
    Rejected(&'static str),
    #[error("extension call outcome unavailable: {0}; effects_may_be_unknown; connection retired")]
    UncertainEffect(&'static str),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolHandle {
    pub connection: Uuid,
    pub generation: Uuid,
    pub name: String,
    pub schema_sha256: String,
    pub definition_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredTool {
    pub handle: ToolHandle,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceHandle {
    pub connection: Uuid,
    pub generation: Uuid,
    pub uri: String,
    pub definition_sha256: String,
}

pub struct McpSession {
    service: Option<RunningService<RoleClient, ClientConfig>>,
    process: Option<ManagedIpcProcess>,
    pub connection: Uuid,
    generation: Uuid,
    allowed_tools: BTreeSet<String>,
    allowed_resource_prefixes: Vec<String>,
    cancellation: CancellationToken,
    pub accounting: Arc<WireAccounting>,
    http_accounting: Option<(
        Arc<std::sync::atomic::AtomicU64>,
        Arc<std::sync::atomic::AtomicU64>,
    )>,
    scoped_secret: Option<Arc<argentum_security::SecretValue>>,
    retired: bool,
}

impl McpSession {
    /// The caller must consume an exact current owner activation approval and
    /// recheck it in pre_resume. Process limits are not an access sandbox.
    pub async fn connect_stdio<F>(
        spec: ProcessSpec,
        limits: ManagedProcessLimits,
        allowed_tools: BTreeSet<String>,
        cancellation: CancellationToken,
        pre_resume: F,
    ) -> Result<Self, ConnectionError>
    where
        F: FnOnce() -> bool + Send,
    {
        if cancellation.is_cancelled() {
            return Err(ConnectionError::Rejected("cancelled_before_spawn"));
        }
        let (mut process, pipes) =
            ManagedIpcProcess::spawn_checked(spec, limits, pre_resume).await?;
        let transport = BoundedStdio::new(pipes.read, pipes.write);
        let accounting = Arc::clone(&transport.accounting);
        let client = ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("argentum", env!("CARGO_PKG_VERSION")),
        );
        let future = client.serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Auto {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                legacy_version: Some(ProtocolVersion::V_2025_11_25),
            },
        );
        let initialized = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(20), future) => result.ok().and_then(Result::ok),
            _ = cancellation.cancelled() => None,
        };
        match initialized {
            Some(service) => Ok(Self {
                service: Some(service),
                process: Some(process),
                connection: Uuid::new_v4(),
                generation: Uuid::new_v4(),
                allowed_tools,
                allowed_resource_prefixes: vec![],
                cancellation,
                accounting,
                http_accounting: None,
                scoped_secret: None,
                retired: false,
            }),
            None => {
                process.terminate_and_wait().await?;
                Err(ConnectionError::Rejected(
                    "initialization_failed_or_cancelled",
                ))
            }
        }
    }

    pub async fn connect_http(
        client: crate::http::ReviewedHttp,
        allowed_tools: BTreeSet<String>,
        cancellation: CancellationToken,
    ) -> Result<Self, ConnectionError> {
        if cancellation.is_cancelled() {
            return Err(ConnectionError::Rejected("cancelled_before_connect"));
        }
        let http_accounting = Some((
            Arc::clone(&client.request_starts),
            Arc::clone(&client.received_bytes),
        ));
        let scoped_secret = client.scoped_secret();
        let config = ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("argentum", env!("CARGO_PKG_VERSION")),
        );
        let future = config.serve_with_lifecycle(
            client.transport(),
            ClientLifecycleMode::Auto {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                legacy_version: Some(ProtocolVersion::V_2025_11_25),
            },
        );
        let service = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(20), future) => result.ok().and_then(Result::ok),
            _ = cancellation.cancelled() => None,
        }.ok_or(ConnectionError::Rejected("http_initialization_failed_or_cancelled"))?;
        Ok(Self {
            service: Some(service),
            process: None,
            connection: Uuid::new_v4(),
            generation: Uuid::new_v4(),
            allowed_tools,
            allowed_resource_prefixes: vec![],
            cancellation,
            accounting: Arc::default(),
            http_accounting,
            scoped_secret,
            retired: false,
        })
    }

    /// Trusted configuration API, never a model action. Changing the scope
    /// invalidates every prior tool/resource handle.
    pub fn set_resource_scope(&mut self, prefixes: Vec<String>) -> Result<(), ConnectionError> {
        if prefixes.len() > 16
            || prefixes.iter().any(|prefix| {
                prefix.is_empty() || prefix.len() > 1024 || prefix.chars().any(char::is_control)
            })
        {
            return Err(ConnectionError::Rejected("invalid_resource_scope"));
        }
        self.allowed_resource_prefixes = prefixes;
        self.generation = Uuid::new_v4();
        Ok(())
    }

    pub async fn discover_resources(&self) -> Result<Vec<ResourceHandle>, ConnectionError> {
        if self.allowed_resource_prefixes.is_empty() {
            return Ok(vec![]);
        }
        self.require_server_capability(true)?;
        let mut output = Vec::new();
        let mut cursor = None;
        let mut seen = BTreeSet::new();
        let mut identities = BTreeSet::new();
        let mut scanned = 0;
        for _ in 0..8 {
            let params = cursor
                .clone()
                .map(|cursor| PaginatedRequestParams::default().with_cursor(Some(cursor)));
            let request = ClientRequest::ListResourcesRequest(ListResourcesRequest {
                method: Default::default(),
                params,
                extensions: Default::default(),
            });
            let result = tokio::select! {
                result = tokio::time::timeout(Duration::from_secs(15), self.service()?.peer().send_request(request)) => result.ok().and_then(Result::ok),
                _ = self.cancellation.cancelled() => None,
            }.ok_or(ConnectionError::Rejected("resource_discovery_failed_or_cancelled"))?;
            let ServerResult::ListResourcesResult(page) = result else {
                return Err(ConnectionError::Rejected("unexpected_resource_response"));
            };
            for resource in page.resources {
                scanned += 1;
                if scanned > MAX_CATALOG_ENTRIES {
                    return Err(ConnectionError::Rejected("resource_capacity_exceeded"));
                }
                let uri = resource.uri.to_string();
                if uri.len() > 2048 || uri.chars().any(char::is_control) {
                    return Err(ConnectionError::Rejected("invalid_resource_identity"));
                }
                if !identities.insert(uri.clone()) {
                    return Err(ConnectionError::Rejected("duplicate_resource_identity"));
                }
                if !self
                    .allowed_resource_prefixes
                    .iter()
                    .any(|prefix| uri.starts_with(prefix))
                {
                    continue;
                }
                let bytes = serde_json::to_vec(&resource)
                    .map_err(|_| ConnectionError::Rejected("invalid_resource_metadata"))?;
                if bytes.len() > 8192 {
                    return Err(ConnectionError::Rejected(
                        "resource_metadata_capacity_exceeded",
                    ));
                }
                output.push(ResourceHandle {
                    connection: self.connection,
                    generation: self.generation,
                    uri,
                    definition_sha256: format!("{:x}", Sha256::digest(bytes)),
                });
            }
            match page.next_cursor {
                None => return Ok(output),
                Some(next) if next.len() <= 512 && seen.insert(next.clone()) => cursor = Some(next),
                Some(_) => return Err(ConnectionError::Rejected("invalid_resource_cursor")),
            }
        }
        Err(ConnectionError::Rejected("resource_page_limit_exceeded"))
    }

    pub async fn discover_resources_page(
        &self,
        cursor: Option<&str>,
    ) -> Result<Value, ConnectionError> {
        let resources = self.discover_resources().await?;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&resources)
                    .map_err(|_| ConnectionError::Rejected("invalid_resource_catalog"))?
            )
        );
        let offset = match cursor {
            None => 0,
            Some(cursor) if cursor.len() <= 1024 => {
                let (connection, generation, binding, offset): (Uuid, Uuid, String, usize) =
                    serde_json::from_str(cursor)
                        .map_err(|_| ConnectionError::Rejected("invalid_resource_page_cursor"))?;
                if connection != self.connection
                    || generation != self.generation
                    || binding != fingerprint
                    || offset > resources.len()
                {
                    return Err(ConnectionError::Rejected(
                        "resource_catalog_changed_discover_again",
                    ));
                }
                offset
            }
            Some(_) => return Err(ConnectionError::Rejected("invalid_resource_page_cursor")),
        };
        let end = (offset + 16).min(resources.len());
        let next = (end < resources.len()).then(|| {
            serde_json::to_string(&(self.connection, self.generation, fingerprint, end))
                .expect("bounded resource cursor")
        });
        Ok(
            serde_json::json!({"resources":&resources[offset..end],"next_cursor":next,"catalog_count":resources.len(),"server_data_is_untrusted":true}),
        )
    }

    pub async fn read_resource(&self, handle: &ResourceHandle) -> Result<Value, ConnectionError> {
        if handle.connection != self.connection || handle.generation != self.generation {
            return Err(ConnectionError::Rejected(
                "stale_or_foreign_resource_handle",
            ));
        }
        let current = self.discover_resources().await?;
        if !current.iter().any(|resource| {
            resource.uri == handle.uri && resource.definition_sha256 == handle.definition_sha256
        }) {
            return Err(ConnectionError::Rejected(
                "resource_changed_or_not_in_scope",
            ));
        }
        let request = ClientRequest::ReadResourceRequest(ReadResourceRequest::new(
            ReadResourceRequestParams::new(handle.uri.clone()),
        ));
        let result = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(15), self.service()?.peer().send_request(request)) => result.ok().and_then(Result::ok),
            _ = self.cancellation.cancelled() => None,
        }.ok_or(ConnectionError::Rejected("resource_read_failed_or_cancelled"))?;
        let result = serde_json::to_value(result)
            .map_err(|_| ConnectionError::Rejected("invalid_resource_result"))?;
        if serde_json::to_vec(&result)
            .map_err(|_| ConnectionError::Rejected("invalid_resource_result"))?
            .len()
            > MAX_RESULT_BYTES
        {
            return Err(ConnectionError::Rejected(
                "resource_result_capacity_exceeded",
            ));
        }
        self.redact_result(result)
    }

    fn service(&self) -> Result<&RunningService<RoleClient, ClientConfig>, ConnectionError> {
        if self.retired || self.cancellation.is_cancelled() {
            return Err(ConnectionError::Rejected("connection_retired"));
        }
        self.service
            .as_ref()
            .ok_or(ConnectionError::Rejected("connection_unavailable"))
    }
    fn require_server_capability(&self, resources: bool) -> Result<(), ConnectionError> {
        let info = self
            .service()?
            .peer_info()
            .ok_or(ConnectionError::Rejected(
                "server_negotiation_state_unavailable",
            ))?;
        if (resources && info.capabilities.resources.is_none())
            || (!resources && info.capabilities.tools.is_none())
        {
            return Err(ConnectionError::Rejected(
                "server_does_not_advertise_requested_capability",
            ));
        }
        Ok(())
    }

    async fn catalog(&self) -> Result<BTreeMap<String, DiscoveredTool>, ConnectionError> {
        self.require_server_capability(false)?;
        let service = self.service()?;
        let mut cursor = None;
        let mut cursors = BTreeSet::new();
        let mut output = BTreeMap::new();
        for _ in 0..8 {
            let request = ClientRequest::ListToolsRequest(ListToolsRequest {
                method: Default::default(),
                params: cursor
                    .clone()
                    .map(|cursor| PaginatedRequestParams::default().with_cursor(Some(cursor))),
                extensions: Default::default(),
            });
            // Low-level request intentionally avoids SDK stale-on-error cache.
            let result = tokio::select! {
                result = tokio::time::timeout(Duration::from_secs(15), service.peer().send_request(request)) => result.ok().and_then(Result::ok),
                _ = self.cancellation.cancelled() => None,
            }.ok_or(ConnectionError::Rejected("discovery_failed_or_cancelled"))?;
            let ServerResult::ListToolsResult(page) = result else {
                return Err(ConnectionError::Rejected("unexpected_catalog_response"));
            };
            for tool in page.tools {
                if output.len() >= MAX_CATALOG_ENTRIES {
                    return Err(ConnectionError::Rejected("catalog_capacity_exceeded"));
                }
                let name = tool.name.to_string();
                if name.is_empty()
                    || name.len() > 128
                    || name.chars().any(char::is_control)
                    || output.contains_key(&name)
                {
                    return Err(ConnectionError::Rejected(
                        "invalid_or_duplicate_tool_identity",
                    ));
                }
                let schema = serde_json::to_value(&tool.input_schema)
                    .map_err(|_| ConnectionError::Rejected("invalid_schema"))?;
                validate_schema(&schema, 0)?;
                let bytes = serde_json::to_vec(&schema)
                    .map_err(|_| ConnectionError::Rejected("invalid_schema"))?;
                if bytes.len() > 8192 {
                    return Err(ConnectionError::Rejected("schema_capacity_exceeded"));
                }
                let definition = serde_json::to_vec(&tool)
                    .map_err(|_| ConnectionError::Rejected("invalid_tool_definition"))?;
                if definition.len() > 16 * 1024 {
                    return Err(ConnectionError::Rejected(
                        "tool_definition_capacity_exceeded",
                    ));
                }
                let description = tool.description.as_deref().unwrap_or_default();
                let mut end = description.len().min(1024);
                while !description.is_char_boundary(end) {
                    end -= 1;
                }
                output.insert(
                    name.clone(),
                    DiscoveredTool {
                        handle: ToolHandle {
                            connection: self.connection,
                            generation: self.generation,
                            name,
                            schema_sha256: format!("{:x}", Sha256::digest(bytes)),
                            definition_sha256: format!("{:x}", Sha256::digest(definition)),
                        },
                        description: description[..end].to_owned(),
                        input_schema: schema,
                    },
                );
            }
            match page.next_cursor {
                None => return Ok(output),
                Some(next) if next.len() <= 512 && cursors.insert(next.clone()) => {
                    cursor = Some(next)
                }
                Some(_) => {
                    return Err(ConnectionError::Rejected(
                        "invalid_or_repeated_catalog_cursor",
                    ))
                }
            }
        }
        Err(ConnectionError::Rejected("catalog_page_limit_exceeded"))
    }

    pub async fn discover(&self) -> Result<Vec<DiscoveredTool>, ConnectionError> {
        Ok(self
            .catalog()
            .await?
            .into_values()
            .filter(|tool| self.allowed_tools.contains(&tool.handle.name))
            .collect())
    }

    pub async fn discover_page(&self, cursor: Option<&str>) -> Result<Value, ConnectionError> {
        let catalog = self.discover().await?;
        let identities = catalog.iter().map(|tool| &tool.handle).collect::<Vec<_>>();
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&identities)
                    .map_err(|_| ConnectionError::Rejected("invalid_catalog"))?
            )
        );
        let offset = match cursor {
            None => 0,
            Some(cursor) if cursor.len() <= 1024 => {
                let parsed: (Uuid, Uuid, String, usize) = serde_json::from_str(cursor)
                    .map_err(|_| ConnectionError::Rejected("invalid_discovery_cursor"))?;
                if parsed.0 != self.connection
                    || parsed.1 != self.generation
                    || parsed.2 != fingerprint
                    || parsed.3 > catalog.len()
                {
                    return Err(ConnectionError::Rejected(
                        "discovery_cursor_stale_restart_discovery",
                    ));
                }
                parsed.3
            }
            Some(_) => {
                return Err(ConnectionError::Rejected(
                    "discovery_cursor_capacity_exceeded",
                ))
            }
        };
        let end = (offset + 4).min(catalog.len());
        let next = (end < catalog.len()).then(|| {
            serde_json::to_string(&(self.connection, self.generation, fingerprint, end))
                .expect("bounded cursor serializes")
        });
        Ok(
            serde_json::json!({"tools":&catalog[offset..end],"next_cursor":next,"catalog_count":catalog.len(),"server_data_is_untrusted":true}),
        )
    }

    pub async fn invoke(
        &self,
        handle: &ToolHandle,
        arguments: Value,
    ) -> Result<Value, ConnectionError> {
        if handle.connection != self.connection || handle.generation != self.generation {
            return Err(ConnectionError::Rejected("stale_or_foreign_tool_handle"));
        }
        if !self.allowed_tools.contains(&handle.name) {
            return Err(ConnectionError::Rejected("tool_not_in_reviewed_scope"));
        }
        let arguments = arguments
            .as_object()
            .ok_or(ConnectionError::Rejected("arguments_must_be_object"))?;
        if serde_json::to_vec(arguments)
            .map_err(|_| ConnectionError::Rejected("invalid_arguments"))?
            .len()
            > 32 * 1024
        {
            return Err(ConnectionError::Rejected("arguments_capacity_exceeded"));
        }
        let catalog = self.catalog().await?;
        let current = catalog
            .get(&handle.name)
            .ok_or(ConnectionError::Rejected("tool_removed_discover_again"))?;
        if &current.handle != handle {
            return Err(ConnectionError::Rejected(
                "tool_definition_changed_discover_again",
            ));
        }
        let request =
            CallToolRequestParams::new(handle.name.clone()).with_arguments(arguments.clone());
        let result = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(30), self.service()?.peer().call_tool(request)) => result.ok().and_then(Result::ok),
            _ = self.cancellation.cancelled() => None,
        };
        let Some(result) = result else {
            self.cancellation.cancel();
            return Err(ConnectionError::UncertainEffect(
                "tool_call_failed_cancelled_or_timed_out",
            ));
        };
        let result = serde_json::to_value(result).map_err(|_| {
            self.cancellation.cancel();
            ConnectionError::UncertainEffect("invalid_tool_result")
        })?;
        if serde_json::to_vec(&result)
            .map_err(|_| ConnectionError::Rejected("invalid_tool_result"))?
            .len()
            > MAX_RESULT_BYTES
        {
            self.cancellation.cancel();
            return Err(ConnectionError::UncertainEffect(
                "tool_result_capacity_exceeded",
            ));
        }
        self.redact_result(result)
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    pub fn accounting_snapshot(&self) -> Value {
        use std::sync::atomic::Ordering;
        if let Some((requests, bytes)) = &self.http_accounting {
            serde_json::json!({"transport":"streamable_http","physical_request_starts":requests.load(Ordering::Relaxed),"received_bytes":bytes.load(Ordering::Relaxed),"scope":"cumulative_current_connection_not_generation_usage"})
        } else {
            serde_json::json!({"transport":"stdio","sent_messages":self.accounting.requests.load(Ordering::Relaxed),"sent_bytes":self.accounting.sent_bytes.load(Ordering::Relaxed),"received_bytes":self.accounting.received_bytes.load(Ordering::Relaxed),"wire_fault":self.accounting.fault.load(Ordering::Relaxed),"scope":"cumulative_current_connection_not_provider_requests"})
        }
    }
    fn redact_result(&self, result: Value) -> Result<Value, ConnectionError> {
        let Some(secret) = &self.scoped_secret else {
            return Ok(result);
        };
        redact_scoped_value(result, secret.expose())
    }
    pub fn protocol_version(&self) -> Option<String> {
        self.service
            .as_ref()
            .and_then(|service| service.peer_info())
            .map(|info| info.protocol_version.to_string())
    }

    pub async fn retire(&mut self) -> Result<(), ConnectionError> {
        self.retired = true;
        self.cancellation.cancel();
        let service_result = if let Some(service) = self.service.take() {
            tokio::time::timeout(Duration::from_secs(5), service.cancel())
                .await
                .map_err(|_| ConnectionError::Rejected("service_teardown_pending"))
                .and_then(|result| {
                    result
                        .map(|_| ())
                        .map_err(|_| ConnectionError::Rejected("service_teardown_failed"))
                })
        } else {
            Ok(())
        };
        // Service cancellation failure must not skip owned process-tree cleanup.
        let process_result = if let Some(process) = self.process.as_mut() {
            match process.terminate_and_wait().await {
                Ok(()) => {
                    self.process.take();
                    Ok(())
                }
                Err(error) => Err(ConnectionError::from(error)),
            }
        } else {
            Ok(())
        };
        process_result.and(service_result)
    }
}

pub(crate) fn redact_scoped_value(value: Value, secret: &str) -> Result<Value, ConnectionError> {
    if secret.is_empty() {
        return Ok(value);
    }
    match value {
        Value::String(value) => Ok(Value::String(value.replace(secret, "[REDACTED]"))),
        Value::Array(values) => values
            .into_iter()
            .map(|value| redact_scoped_value(value, secret))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => {
            let mut output = serde_json::Map::new();
            for (name, value) in values {
                if output
                    .insert(
                        name.replace(secret, "[REDACTED]"),
                        redact_scoped_value(value, secret)?,
                    )
                    .is_some()
                {
                    return Err(ConnectionError::UncertainEffect(
                        "credential_redaction_collided_result_unavailable",
                    ));
                }
            }
            Ok(Value::Object(output))
        }
        value => Ok(value),
    }
}

fn validate_schema(schema: &Value, depth: usize) -> Result<(), ConnectionError> {
    if depth > 12 {
        return Err(ConnectionError::Rejected("schema_depth_exceeded"));
    }
    if depth == 0
        && (!schema.is_object() || schema.get("type").and_then(Value::as_str) != Some("object"))
    {
        return Err(ConnectionError::Rejected("tool_schema_must_be_object"));
    }
    match schema {
        Value::Object(fields) => {
            if fields.contains_key("$ref") || fields.contains_key("$dynamicRef") {
                return Err(ConnectionError::Rejected(
                    "referenced_schema_unsupported_inline_schema_required",
                ));
            }
            if let Some(kind) = fields.get("type") {
                let valid = |value: &Value| {
                    value.as_str().is_some_and(|name| {
                        matches!(
                            name,
                            "object"
                                | "array"
                                | "string"
                                | "number"
                                | "integer"
                                | "boolean"
                                | "null"
                        )
                    })
                };
                if !valid(kind)
                    && !kind
                        .as_array()
                        .is_some_and(|values| !values.is_empty() && values.iter().all(valid))
                {
                    return Err(ConnectionError::Rejected("malformed_schema_type"));
                }
            }
            if let Some(required) = fields.get("required") {
                let values = required
                    .as_array()
                    .ok_or(ConnectionError::Rejected("malformed_schema_required"))?;
                let mut names = BTreeSet::new();
                if values.iter().any(|value| {
                    value
                        .as_str()
                        .is_none_or(|name| name.is_empty() || !names.insert(name))
                }) {
                    return Err(ConnectionError::Rejected("malformed_schema_required"));
                }
            }
            for key in ["properties", "patternProperties", "$defs", "definitions"] {
                if fields.get(key).is_some_and(|value| !value.is_object()) {
                    return Err(ConnectionError::Rejected("malformed_schema_properties"));
                }
            }
            for key in ["oneOf", "anyOf", "allOf", "prefixItems"] {
                if fields.get(key).is_some_and(|value| {
                    !value.as_array().is_some_and(|values| {
                        !values.is_empty()
                            && values
                                .iter()
                                .all(|value| value.is_object() || value.is_boolean())
                    })
                }) {
                    return Err(ConnectionError::Rejected("malformed_schema_composition"));
                }
            }
            for key in [
                "minLength",
                "maxLength",
                "minItems",
                "maxItems",
                "minProperties",
                "maxProperties",
            ] {
                if fields
                    .get(key)
                    .is_some_and(|value| value.as_u64().is_none())
                {
                    return Err(ConnectionError::Rejected("malformed_schema_bound"));
                }
            }
            for (name, value) in fields {
                match name.as_str() {
                    "properties" | "patternProperties" | "$defs" | "definitions"
                    | "dependentSchemas" => {
                        let map = value
                            .as_object()
                            .ok_or(ConnectionError::Rejected("malformed_schema_properties"))?;
                        for schema in map.values() {
                            validate_schema(schema, depth + 1)?;
                        }
                    }
                    "oneOf" | "anyOf" | "allOf" | "prefixItems" => {
                        let list = value
                            .as_array()
                            .ok_or(ConnectionError::Rejected("malformed_schema_composition"))?;
                        for schema in list {
                            validate_schema(schema, depth + 1)?;
                        }
                    }
                    "items"
                    | "contains"
                    | "additionalProperties"
                    | "unevaluatedProperties"
                    | "unevaluatedItems"
                    | "propertyNames"
                    | "not"
                    | "if"
                    | "then"
                    | "else" => validate_schema(value, depth + 1)?,
                    _ => {} // Annotation/enum/const data is not executable schema.
                }
            }
        }
        Value::Bool(_) if depth > 0 => {}
        _ => {
            return Err(ConnectionError::Rejected(
                "schema_must_be_object_or_boolean",
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_credentials_cannot_be_echoed_into_model_results() {
        let result = redact_scoped_value(serde_json::json!({"content":[{"text":"secret-synthetic\n"}],"structuredContent":{"secret-synthetic":"secret-synthetic"}}),"secret-synthetic").unwrap();
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("secret-synthetic"));
        assert!(redact_scoped_value(
            serde_json::json!({"secret-synthetic":1,"[REDACTED]":2}),
            "secret-synthetic"
        )
        .is_err());
    }
    #[test]
    fn malformed_or_external_schema_does_not_become_authority() {
        assert!(validate_schema(&serde_json::json!({"type":"object","properties":{"type":{"type":"string"},"required":{"type":"string"}},"enum":[{"type":23}]}),0).is_ok(),"property names and enum values are data, not schema keywords");
        assert!(validate_schema(
            &serde_json::json!({"type":"object","properties":{"path":{"type":"string"}}}),
            0
        )
        .is_ok());
        for schema in [
            serde_json::json!(null),
            serde_json::json!({"type":"array"}),
            serde_json::json!({"type":"object","$ref":"https://example.com/schema"}),
            serde_json::json!({"type":"object","properties":"not an object"}),
            serde_json::json!({"type":"object","properties":{"path":{"type":23}}}),
            serde_json::json!({"type":"object","required":["x","x"]}),
            serde_json::json!({"type":"object","oneOf":"not an array"}),
        ] {
            assert!(validate_schema(&schema, 0).is_err());
        }
    }
}
