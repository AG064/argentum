//! Optional extension broker using the existing store, authority and lifecycle.
use argentum_domain::{
    extensions::{ExtensionDefinition, ExtensionOperation, ExtensionRecord, ExtensionTransport},
    ProjectId, Sha256Digest, ToolRequest,
};
use argentum_extensions::{
    configuration::{admit, approval_binding},
    connection::McpSession,
    lifecycle::ExtensionLifecycle,
};
use argentum_store::Store;
use argentum_tools::{ExtensionExecutor, ToolContext, ToolError, ToolResult};
use argentum_workspaces::WorkspaceManager;
use async_trait::async_trait;
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn mcp_failure(error: argentum_extensions::connection::ConnectionError) -> ToolError {
    use argentum_extensions::connection::ConnectionError;
    let (code, effects_unknown) = match error {
        ConnectionError::Rejected(code) => (code, false),
        ConnectionError::UncertainEffect(code) => (code, true),
        ConnectionError::Process(_) => (
            "managed_extension_process_unavailable_use_local_diagnostics",
            false,
        ),
    };
    ToolError::ExtensionFailure {
        code,
        effects_unknown,
    }
}

#[cfg(all(test, windows))]
mod image_authority_tests {
    use super::*;
    #[tokio::test]
    async fn captured_pixels_require_current_run_context_revision_and_policy_on_dispatch() {
        let (root, runtime) = crate::tests::composed_runtime().await;
        let package = root.path().join("package");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("plugin.json"), serde_json::to_vec(&json!({"$schema":argentum_extensions::package::PLUGIN_SCHEMA,"name":"image-control","version":"1","description":"Synthetic image authority control","license":"MIT"})).unwrap()).unwrap();
        std::fs::write(package.join("LICENSE"), "Synthetic test license").unwrap();
        let program = std::env::current_exe().unwrap();
        std::fs::write(package.join("mcp.json"),serde_json::to_vec(&json!({"$schema":argentum_extensions::package::MCP_SCHEMA,"mcpServers":{"stdio":{"type":"stdio","command":program,"args":[]}}})).unwrap()).unwrap();
        let inspection = argentum_extensions::package::inspect(&package).unwrap();
        let media = root.path().join("media");
        std::fs::create_dir(&media).unwrap();
        argentum_execution::ensure_owner_only_directory(&media).unwrap();
        let program_lease =
            argentum_execution::VerifiedFileLease::open(&program, 256 * 1024 * 1024).unwrap();
        let definition = ExtensionDefinition {
            namespace: "image-control.stdio".into(),
            package_root: package.to_string_lossy().into_owned(),
            package_sha256: Sha256Digest(inspection.digest),
            source_id: "synthetic".into(),
            source_revision: "v1".into(),
            transport: ExtensionTransport::Stdio {
                program: program.to_string_lossy().into_owned(),
                program_sha256: Sha256Digest(program_lease.sha256().into()),
                args: vec![],
                cwd: runtime.workspace.root().to_string_lossy().into_owned(),
                environment: Default::default(),
                inputs: Default::default(),
            },
            allowed_tools: vec!["browser_screenshot".into()],
            resource_prefixes: vec![],
            trust: argentum_domain::extensions::ExtensionTrust::ReviewedCurrentUserCode,
            image_scope: Some(argentum_domain::extensions::ExtensionImageScope {
                directory: media.to_string_lossy().into_owned(),
                origins: vec!["http://127.0.0.1:18080".into()],
            }),
        };
        let admitted = admit(&definition, runtime.workspace.root()).unwrap();
        // Test-owned isolated store fixture, not a human approval or execution.
        // Actual opaque owner activation is covered by the CLI controls.
        let mut record = ExtensionRecord {
            definition,
            revision: Uuid::new_v4(),
            active: true,
            approval_binding: Some(admitted.binding),
        };
        runtime
            .store
            .save_extension_record(runtime.project_id, &record, None)
            .unwrap();
        let mut policy = runtime.execution_policy().unwrap();
        policy
            .capability_enabled
            .insert("extension.external".into(), true);
        runtime
            .store
            .save_execution_policy(runtime.project_id, &policy)
            .unwrap();
        let run = Uuid::new_v4();
        let context = Uuid::new_v4();
        runtime
            .extension_owner()
            .image_contexts
            .lock()
            .unwrap()
            .insert(format!("image-control.stdio@{run}"), context);
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[20, 30, 40])
                .unwrap();
        }
        use sha2::Digest;
        let sha = format!("{:x}", sha2::Sha256::digest(&png));
        let image =
            argentum_providers::ModelImage::from_png(&png, &sha, "http://127.0.0.1:18080".into())
                .unwrap()
                .bind_extension(argentum_providers::ExtensionImageBinding {
                    namespace: "image-control.stdio".into(),
                    revision: record.revision,
                    run_id: run,
                    context_id: context,
                });
        let mut request = argentum_providers::ModelRequest {
            continuation_scope: Some(argentum_providers::ContinuationScope {
                session_id: Uuid::new_v4(),
                run_id: run,
            }),
            require_tool_call: false,
            output_limit: 0,
            model: "fixture".into(),
            system: None,
            history: vec![],
            prompt: "synthetic".into(),
            tools: vec![],
            tool_exchanges: vec![argentum_providers::ModelToolExchange {
                assistant_content: String::new(),
                assistant_reasoning: String::new(),
                assistant_continuation: None,
                chatgpt_continuation: None,
                calls: vec![],
                results: vec![argentum_providers::ModelToolResult {
                    images: vec![image],
                    call_id: "capture".into(),
                    content: "scoped capture".into(),
                }],
            }],
        };
        runtime.ensure_extension_images_current(&request).unwrap();
        let actual = super::super::model_request_projection_bytes(&request);
        let bytes = request.tool_exchanges[0].results[0].images[0]
            .data_url()
            .len();
        let mut no_pixels = request.clone();
        no_pixels.tool_exchanges[0].results[0].images.clear();
        assert_eq!(
            actual,
            super::super::model_request_projection_bytes(&no_pixels) + bytes
        );
        request.continuation_scope.as_mut().unwrap().run_id = Uuid::new_v4();
        assert!(runtime.ensure_extension_images_current(&request).is_err());
        request.continuation_scope.as_mut().unwrap().run_id = run;
        policy
            .capability_enabled
            .insert("extension.external".into(), false);
        runtime
            .store
            .save_execution_policy(runtime.project_id, &policy)
            .unwrap();
        assert!(runtime.ensure_extension_images_current(&request).is_err());
        policy
            .capability_enabled
            .insert("extension.external".into(), true);
        runtime
            .store
            .save_execution_policy(runtime.project_id, &policy)
            .unwrap();
        let previous = record.revision;
        record.revision = Uuid::new_v4();
        runtime
            .store
            .save_extension_record(runtime.project_id, &record, Some(previous))
            .unwrap();
        assert!(runtime.ensure_extension_images_current(&request).is_err());
        let previous = record.revision;
        record.active = false;
        record.approval_binding = None;
        record.revision = Uuid::new_v4();
        runtime
            .store
            .save_extension_record(runtime.project_id, &record, Some(previous))
            .unwrap();
        assert!(runtime.ensure_extension_images_current(&request).is_err());
        runtime.shutdown().await.unwrap();
    }
}

pub(crate) struct ExtensionOwner {
    store: Store,
    project: ProjectId,
    workspace: WorkspaceManager,
    lifecycle: AsyncMutex<ExtensionLifecycle>,
    admission: AsyncMutex<()>,
    tokens: Mutex<BTreeMap<String, CancellationToken>>,
    secrets: Mutex<Option<Arc<dyn argentum_platform::SecretStore>>>,
    image_contexts: Mutex<BTreeMap<String, Uuid>>,
}

impl ExtensionOwner {
    pub fn new(store: Store, project: ProjectId, workspace: WorkspaceManager) -> Self {
        Self {
            store,
            project,
            workspace,
            lifecycle: AsyncMutex::new(ExtensionLifecycle::default()),
            admission: AsyncMutex::new(()),
            tokens: Mutex::new(BTreeMap::new()),
            secrets: Mutex::new(None),
            image_contexts: Mutex::new(BTreeMap::new()),
        }
    }
    fn current(&self, namespace: &str) -> Result<ExtensionRecord, ToolError> {
        let record = self
            .store
            .extension_record(self.project, namespace)
            .map_err(|_| ToolError::Execution("extension_store_unavailable".into()))?
            .ok_or_else(|| {
                ToolError::InvalidInput(
                    "extension_not_configured_use_normal_extensions_commands".into(),
                )
            })?;
        let binding = approval_binding(&record.definition, self.workspace.root())
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        if !record.active || record.approval_binding.as_ref() != Some(&binding) {
            return Err(ToolError::Execution(
                "extension_not_owner_activated_or_binding_changed".into(),
            ));
        }
        Ok(record)
    }
    pub async fn retire_namespace(&self, namespace: &str) -> Result<(), ToolError> {
        let keys = {
            let tokens = self
                .tokens
                .lock()
                .map_err(|_| ToolError::Execution("extension_state_unavailable".into()))?;
            let keys = tokens
                .keys()
                .filter(|key| key.starts_with(&format!("{namespace}@")))
                .cloned()
                .collect::<Vec<_>>();
            for key in &keys {
                if let Some(token) = tokens.get(key) {
                    token.cancel();
                }
            }
            keys
        };
        for key in keys {
            self.lifecycle
                .lock()
                .await
                .retire(&key)
                .await
                .map_err(|error| ToolError::Execution(error.to_string()))?;
            self.tokens
                .lock()
                .map_err(|_| ToolError::Execution("extension_state_unavailable".into()))?
                .remove(&key);
        }
        Ok(())
    }
    pub async fn shutdown(&self) -> Result<(), ToolError> {
        if let Ok(tokens) = self.tokens.lock() {
            for token in tokens.values() {
                token.cancel();
            }
        }
        self.lifecycle
            .lock()
            .await
            .shutdown()
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
    pub async fn revoke_all(&self) -> Result<(), ToolError> {
        let records = self
            .store
            .extension_records(self.project)
            .map_err(|_| ToolError::Execution("extension_store_unavailable".into()))?;
        for record in records {
            if record.active {
                let mut revoked = record.clone();
                revoked.active = false;
                revoked.approval_binding = None;
                revoked.revision = Uuid::new_v4();
                self.store
                    .save_extension_record(self.project, &revoked, Some(record.revision))
                    .map_err(|_| {
                        ToolError::Execution("extension_revocation_store_failed".into())
                    })?;
            }
            self.retire_namespace(&record.definition.namespace).await?;
        }
        Ok(())
    }
    async fn session(
        &self,
        record: &ExtensionRecord,
        run: Uuid,
        parent: &CancellationToken,
    ) -> Result<Arc<AsyncMutex<Option<McpSession>>>, ToolError> {
        let key = format!("{}@{run}", record.definition.namespace);
        let _admission = self.admission.lock().await;
        if parent.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        if self
            .tokens
            .lock()
            .map_err(|_| ToolError::Execution("extension_state_unavailable".into()))?
            .get(&key)
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(ToolError::Execution(
                "retired_extension_session_is_not_reconnected_or_replayed".into(),
            ));
        }
        if let Some((session, _)) = self
            .lifecycle
            .lock()
            .await
            .connection(&key, record.revision)
        {
            return Ok(session);
        }
        self.lifecycle
            .lock()
            .await
            .retire(&key)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let admitted = admit(&record.definition, self.workspace.root())
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        let cancellation = parent.child_token();
        self.tokens
            .lock()
            .map_err(|_| ToolError::Execution("extension_state_unavailable".into()))?
            .insert(key.clone(), cancellation.clone());
        let current = self.current(&record.definition.namespace)?;
        if current.revision != record.revision {
            return Err(ToolError::Execution(
                "extension_configuration_changed".into(),
            ));
        }
        let allowed = record.definition.allowed_tools.iter().cloned().collect();
        let mut session = match &record.definition.transport {
            ExtensionTransport::Stdio { .. } => {
                let spec = admitted
                    .program_spec
                    .clone()
                    .ok_or_else(|| ToolError::Execution("stdio_plan_unavailable".into()))?;
                let limits =
                    argentum_extensions::default_process_limits().map_err(ToolError::Process)?;
                McpSession::connect_stdio(spec, limits, allowed, cancellation.clone(), || {
                    !parent.is_cancelled()
                        && admitted.still_current()
                        && self
                            .current(&record.definition.namespace)
                            .is_ok_and(|current| current.revision == record.revision)
                })
                .await
            }
            ExtensionTransport::StreamableHttp {
                endpoint,
                addresses,
                local_demo,
                credential_account,
            } => {
                let addresses = addresses
                    .iter()
                    .map(|address| address.parse())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| ToolError::InvalidInput("invalid_reviewed_address".into()))?;
                let credential = if let Some(account) = credential_account {
                    let key = extension_credential_key(self.project, &record.definition)?;
                    let store = self
                        .secrets
                        .lock()
                        .map_err(|_| {
                            ToolError::Execution("secure_extension_store_unavailable".into())
                        })?
                        .clone()
                        .ok_or_else(|| {
                            ToolError::Execution(
                                "secure_extension_credential_binding_not_attached".into(),
                            )
                        })?;
                    let secret = store
                        .get(&key)
                        .map_err(|_| {
                            ToolError::Execution("secure_extension_credential_unavailable".into())
                        })?
                        .ok_or_else(|| {
                            ToolError::Execution(
                                "secure_extension_key_missing_use_extensions_key_in_owner_console"
                                    .into(),
                            )
                        })?;
                    let origin = reqwest_origin(endpoint)?;
                    Some(argentum_extensions::http::ScopedCredential {
                        origin,
                        account: account.clone(),
                        secret: Arc::new(secret),
                    })
                } else {
                    None
                };
                let client = argentum_extensions::http::ReviewedHttp::new(
                    endpoint,
                    &addresses,
                    *local_demo,
                    credential,
                )
                .map_err(|_| ToolError::InvalidInput("invalid_HTTP_binding".into()))?;
                McpSession::connect_http(client, allowed, cancellation.clone()).await
            }
            ExtensionTransport::Skills => {
                return Err(ToolError::InvalidInput(
                    "skills_only_extension_has_no_MCP_server".into(),
                ))
            }
        }
        .map_err(mcp_failure)?;
        session
            .set_resource_scope(record.definition.resource_prefixes.clone())
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        self.lifecycle
            .lock()
            .await
            .mount(key.clone(), record.revision, session, admitted)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        self.lifecycle
            .lock()
            .await
            .connection(&key, record.revision)
            .map(|(session, _)| session)
            .ok_or_else(|| ToolError::Execution("extension_mount_unavailable".into()))
    }
}

fn reqwest_origin(endpoint: &str) -> Result<String, ToolError> {
    // Use the same URL parser as the transport, without a provider-derived alias.
    argentum_extensions::http::endpoint_origin(endpoint)
        .map_err(|_| ToolError::InvalidInput("invalid_extension_origin".into()))
}

fn extension_credential_key(
    project: ProjectId,
    definition: &ExtensionDefinition,
) -> Result<String, ToolError> {
    let ExtensionTransport::StreamableHttp {
        endpoint,
        credential_account: Some(account),
        ..
    } = &definition.transport
    else {
        return Err(ToolError::InvalidInput(
            "extension_has_no_secure_HTTP_account_binding".into(),
        ));
    };
    let origin = reqwest_origin(endpoint)?;
    let bytes = serde_json::to_vec(&(
        "argentum-extension-credential/v1",
        project,
        &definition.namespace,
        origin,
        account,
    ))
    .map_err(|_| ToolError::InvalidInput("invalid_credential_binding".into()))?;
    use sha2::Digest;
    Ok(format!(
        "argentum.extension.{:x}",
        sha2::Sha256::digest(bytes)
    ))
}

#[async_trait]
impl ExtensionExecutor for ExtensionOwner {
    async fn execute(
        &self,
        operation: ExtensionOperation,
        request: &ToolRequest,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        if context.cancellation.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        let admitted_record = match &operation {
            ExtensionOperation::List => None,
            ExtensionOperation::LoadSkill { namespace, .. }
            | ExtensionOperation::DiscoverTools { namespace, .. }
            | ExtensionOperation::Invoke { namespace, .. }
            | ExtensionOperation::DiscoverResources { namespace, .. }
            | ExtensionOperation::ReadResource { namespace, .. } => Some(self.current(namespace)?),
        };
        let (namespace, mut output) = match operation {
            ExtensionOperation::List => {
                let records = self
                    .store
                    .extension_records(self.project)
                    .map_err(|_| ToolError::Execution("extension_store_unavailable".into()))?;
                let mut values = Vec::new();
                use sha2::Digest;
                for record in records {
                    let inspected = argentum_extensions::package::inspect(Path::new(
                        &record.definition.package_root,
                    ))
                    .ok()
                    .filter(|inspection| inspection.digest == record.definition.package_sha256.0);
                    values.push(json!({"namespace":record.definition.namespace,
                        "source_id_sha256":format!("{:x}",sha2::Sha256::digest(record.definition.source_id.as_bytes())),
                        "source_revision_sha256":format!("{:x}",sha2::Sha256::digest(record.definition.source_revision.as_bytes())),
                        "package_sha256":record.definition.package_sha256,"active":record.active,"trust":record.definition.trust,
                        "skill_paths":inspected.as_ref().map(|inspection|&inspection.skill_paths),"package_current":inspected.is_some()}));
                }
                (
                    None,
                    json!({"extensions":values,"descriptions_are_untrusted":true}),
                )
            }
            ExtensionOperation::LoadSkill { namespace, path } => {
                let record = self.current(&namespace)?;
                let admitted = admit(&record.definition, self.workspace.root())
                    .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
                let inspection = argentum_extensions::package::inspect(Path::new(
                    &record.definition.package_root,
                ))
                .map_err(|_| ToolError::InvalidInput("package_unavailable".into()))?;
                if !inspection.skill_paths.contains(&path) {
                    return Err(ToolError::InvalidInput(
                        "skill_not_in_pinned_package".into(),
                    ));
                }
                let path = Path::new(&record.definition.package_root).join(
                    argentum_extensions::package::relative_path(&path)
                        .map_err(|_| ToolError::InvalidInput("invalid_skill_path".into()))?,
                );
                use std::io::Read;
                let mut body = String::new();
                std::fs::File::open(path)
                    .map_err(|_| ToolError::Execution("skill_unavailable".into()))?
                    .take(32 * 1024 + 1)
                    .read_to_string(&mut body)
                    .map_err(|_| ToolError::InvalidInput("skill_must_be_bounded_UTF8".into()))?;
                if body.len() > 32 * 1024 || !admitted.still_current() {
                    return Err(ToolError::InvalidInput(
                        "skill_capacity_or_identity_changed".into(),
                    ));
                }
                (
                    Some(namespace),
                    json!({"kind":"untrusted_extension_skill","content":body,"can_grant_authority":false}),
                )
            }
            ExtensionOperation::DiscoverTools { namespace, cursor } => {
                let record = self.current(&namespace)?;
                let session = self
                    .session(&record, request.run_id, &context.cancellation)
                    .await?;
                let session = session.lock().await;
                let session = session.as_ref().ok_or(ToolError::Cancelled)?;
                (
                    Some(namespace),
                    session
                        .discover_page(cursor.as_deref())
                        .await
                        .map_err(mcp_failure)?,
                )
            }
            ExtensionOperation::Invoke {
                namespace,
                handle,
                arguments,
            } => {
                let record = self.current(&namespace)?;
                let session = self
                    .session(&record, request.run_id, &context.cancellation)
                    .await?;
                let handle = serde_json::from_value(handle).map_err(|_| {
                    ToolError::InvalidInput("invalid_tool_handle_discover_again".into())
                })?;
                let session = session.lock().await;
                let session = session.as_ref().ok_or(ToolError::Cancelled)?;
                let result = match session.invoke(&handle, arguments).await {
                    Ok(result) => result,
                    Err(error) => {
                        if matches!(
                            error,
                            argentum_extensions::connection::ConnectionError::UncertainEffect(_)
                        ) {
                            let mut held = record.clone();
                            held.active = false;
                            held.approval_binding = None;
                            held.revision = Uuid::new_v4();
                            self.store
                                .save_extension_record(self.project, &held, Some(record.revision))
                                .map_err(|_| {
                                    ToolError::Execution(
                                        "extension_uncertain_effect_hold_could_not_be_recorded"
                                            .into(),
                                    )
                                })?;
                        }
                        return Err(mcp_failure(error));
                    }
                };
                if result.get("isError").and_then(serde_json::Value::as_bool) == Some(true) {
                    let detail = serde_json::to_string(&result)
                        .unwrap_or_else(|_| "result unavailable".into());
                    let mut end = detail.len().min(4096);
                    while !detail.is_char_boundary(end) {
                        end -= 1;
                    }
                    return Err(ToolError::ExtensionReportedFailure(
                        detail[..end].to_owned(),
                    ));
                }
                if record.definition.image_scope.is_some() && handle.name == "browser_open" {
                    let image_context = result
                        .get("structuredContent")
                        .and_then(|value| value.get("context_id"))
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok())
                        .ok_or_else(|| {
                            ToolError::Execution("browser_context_identity_unavailable".into())
                        })?;
                    self.image_contexts
                        .lock()
                        .map_err(|_| {
                            ToolError::Execution("image_context_state_unavailable".into())
                        })?
                        .insert(format!("{namespace}@{}", request.run_id), image_context);
                }
                (Some(namespace), result)
            }
            ExtensionOperation::DiscoverResources { namespace, cursor } => {
                let record = self.current(&namespace)?;
                let session = self
                    .session(&record, request.run_id, &context.cancellation)
                    .await?;
                let session = session.lock().await;
                let session = session.as_ref().ok_or(ToolError::Cancelled)?;
                (
                    Some(namespace),
                    session
                        .discover_resources_page(cursor.as_deref())
                        .await
                        .map_err(mcp_failure)?,
                )
            }
            ExtensionOperation::ReadResource { namespace, handle } => {
                let record = self.current(&namespace)?;
                let session = self
                    .session(&record, request.run_id, &context.cancellation)
                    .await?;
                let handle = serde_json::from_value(handle).map_err(|_| {
                    ToolError::InvalidInput("invalid_resource_handle_discover_again".into())
                })?;
                let session = session.lock().await;
                let session = session.as_ref().ok_or(ToolError::Cancelled)?;
                (
                    Some(namespace),
                    session.read_resource(&handle).await.map_err(mcp_failure)?,
                )
            }
        };
        if let Some(namespace) = namespace {
            let current = self.current(&namespace)?;
            if admitted_record.as_ref() != Some(&current) {
                return Err(ToolError::Execution(
                    "extension_configuration_changed_during_operation_result_not_current".into(),
                ));
            }
            if let Some((session, _)) = self
                .lifecycle
                .lock()
                .await
                .connection(&format!("{namespace}@{}", request.run_id), current.revision)
            {
                if let Some(session) = session.lock().await.as_ref() {
                    if let Some(fields) = output.as_object_mut() {
                        fields.insert(
                            "_transport_accounting".into(),
                            session.accounting_snapshot(),
                        );
                    }
                }
            }
        }
        let output = serde_json::to_string(&output)
            .map_err(|_| ToolError::Execution("invalid_extension_result".into()))?;
        if output.len() > 64 * 1024 {
            return Err(ToolError::Execution(
                "extension_result_capacity_exceeded".into(),
            ));
        }
        if context.cancellation.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        Ok(ToolResult {
            summary: "Reviewed extension result; not host authority or verification evidence"
                .into(),
            output,
            evidence: Default::default(),
        })
    }
}

impl super::RuntimeService {
    pub(crate) async fn revoke_extensions(&self) -> Result<(), super::RuntimeError> {
        self.extension_owner().revoke_all().await?;
        Ok(())
    }
    pub(crate) fn extension_images(
        &self,
        input: &argentum_domain::ToolInput,
        run_id: Uuid,
        output: &str,
    ) -> Result<Vec<argentum_providers::ModelImage>, super::RuntimeError> {
        let argentum_domain::ToolInput::Extension {
            operation:
                ExtensionOperation::Invoke {
                    namespace, handle, ..
                },
        } = input
        else {
            return Ok(vec![]);
        };
        if handle.get("name").and_then(serde_json::Value::as_str) != Some("browser_screenshot") {
            return Ok(vec![]);
        }
        let owner = self.extension_owner();
        let record = owner.current(namespace)?;
        let Some(scope) = &record.definition.image_scope else {
            return Err(
                ToolError::Execution("browser_image_scope_not_owner_configured".into()).into(),
            );
        };
        let profile = self
            .store
            .selected_provider_profile(self.project_id)?
            .ok_or_else(|| ToolError::Execution("selected_provider_missing".into()))?;
        if profile.id != "chatgpt" {
            return Err(ToolError::Execution(
                "selected_route_has_no_qualified_image_input_path_no_provider_switch_performed"
                    .into(),
            )
            .into());
        }
        let value: serde_json::Value = serde_json::from_str(output)
            .map_err(|_| ToolError::Execution("invalid_image_artifact_result".into()))?;
        let result = value
            .get("structuredContent")
            .ok_or_else(|| ToolError::Execution("structured_image_identity_missing".into()))?;
        let context_id = result
            .get("context_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| ToolError::Execution("invalid_image_context".into()))?;
        if owner
            .image_contexts
            .lock()
            .map_err(|_| super::RuntimeError::StateLockPoisoned)?
            .get(&format!("{namespace}@{run_id}"))
            != Some(&context_id)
        {
            return Err(ToolError::Execution("image_context_is_stale_or_foreign".into()).into());
        }
        let origin = result
            .get("origin")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ToolError::Execution("image_origin_missing".into()))?;
        if !scope.origins.iter().any(|allowed| allowed == origin) {
            return Err(ToolError::Execution("image_origin_not_in_reviewed_scope".into()).into());
        }
        let name = result
            .get("artifact")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ToolError::Execution("image_artifact_missing".into()))?;
        let stem = name
            .strip_suffix(".png")
            .ok_or_else(|| ToolError::Execution("image_artifact_must_be_PNG".into()))?;
        if Uuid::parse_str(stem).is_err() || stem.contains(['/', '\\', ':']) {
            return Err(ToolError::Execution("invalid_image_artifact_identity".into()).into());
        }
        let expected = result
            .get("sha256")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ToolError::Execution("image_hash_missing".into()))?;
        let path = Path::new(&scope.directory).join(name);
        let lease = argentum_extensions::open_image_lease(&path)
            .map_err(|_| ToolError::Execution("image_artifact_unavailable".into()))?;
        if lease.sha256() != expected {
            return Err(ToolError::Execution("image_artifact_changed".into()).into());
        }
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(lease.path())
            .map_err(|_| ToolError::Execution("image_read_failed".into()))?
            .take(128 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ToolError::Execution("image_read_failed".into()))?;
        let image = argentum_providers::ModelImage::from_png(&bytes, expected, origin.to_owned())?
            .bind_extension(argentum_providers::ExtensionImageBinding {
                namespace: namespace.clone(),
                revision: record.revision,
                run_id,
                context_id,
            });
        if owner.current(namespace)? != record {
            return Err(
                ToolError::Execution("image_configuration_changed_during_read".into()).into(),
            );
        }
        Ok(vec![image])
    }
    pub(crate) fn ensure_extension_images_current(
        &self,
        request: &argentum_providers::ModelRequest,
    ) -> Result<(), super::RuntimeError> {
        for image in request
            .tool_exchanges
            .iter()
            .flat_map(|exchange| &exchange.results)
            .flat_map(|result| &result.images)
        {
            let binding = image
                .extension_binding()
                .ok_or_else(|| ToolError::Execution("unbound_image_cannot_be_dispatched".into()))?;
            if request
                .continuation_scope
                .is_none_or(|scope| scope.run_id != binding.run_id)
                || self
                    .execution_policy()?
                    .capability_enabled
                    .get("extension.external")
                    != Some(&true)
            {
                return Err(ToolError::Execution(
                    "image_run_or_current_policy_binding_changed".into(),
                )
                .into());
            }
            let owner = self.extension_owner();
            let record = owner.current(&binding.namespace)?;
            if record.revision != binding.revision
                || record
                    .definition
                    .image_scope
                    .as_ref()
                    .is_none_or(|scope| !scope.origins.contains(&image.origin))
                || owner
                    .image_contexts
                    .lock()
                    .map_err(|_| super::RuntimeError::StateLockPoisoned)?
                    .get(&format!("{}@{}", binding.namespace, binding.run_id))
                    != Some(&binding.context_id)
            {
                return Err(ToolError::Execution(
                    "image_producer_or_context_binding_changed_no_dispatch".into(),
                )
                .into());
            }
        }
        Ok(())
    }
    pub fn attach_extension_secret_store(
        &self,
        store: Arc<dyn argentum_platform::SecretStore>,
    ) -> Result<(), super::RuntimeError> {
        *self
            .extension_owner()
            .secrets
            .lock()
            .map_err(|_| super::RuntimeError::StateLockPoisoned)? = Some(store);
        Ok(())
    }
    pub async fn set_extension_secret(
        &self,
        namespace: &str,
        value: argentum_security::SecretValue,
    ) -> Result<(), super::RuntimeError> {
        self.disable_extension(namespace, false).await?;
        let record = self
            .store
            .extension_record(self.project_id, namespace)?
            .ok_or_else(|| ToolError::InvalidInput("extension_not_configured".into()))?;
        let key = extension_credential_key(self.project_id, &record.definition)?;
        let store = self
            .extension_owner()
            .secrets
            .lock()
            .map_err(|_| super::RuntimeError::StateLockPoisoned)?
            .clone()
            .ok_or_else(|| ToolError::Execution("secure_extension_store_unavailable".into()))?;
        store.set(&key, value).map_err(|_| {
            ToolError::Execution(
                "secure_extension_key_save_failed_activation_remains_revoked".into(),
            )
        })?;
        Ok(())
    }
    pub(crate) fn extension_owner(&self) -> Arc<ExtensionOwner> {
        Arc::clone(self.extension_owner.get_or_init(|| {
            Arc::new(ExtensionOwner::new(
                self.store.clone(),
                self.project_id,
                self.workspace.clone(),
            ))
        }))
    }
    pub fn extension_records(&self) -> Result<Vec<ExtensionRecord>, super::RuntimeError> {
        Ok(self.store.extension_records(self.project_id)?)
    }
    pub fn extension_readiness(&self) -> Result<Vec<serde_json::Value>, super::RuntimeError> {
        let policy = self.execution_policy()?;
        let selected = self
            .provider_profiles()?
            .into_iter()
            .find(|profile| profile.selected);
        let mut output = Vec::new();
        for record in self.extension_records()? {
            let inspected = admit(&record.definition, self.workspace.root());
            let issue = match &inspected {
                Err(error) => Some(error.to_string()),
                Ok(admitted) if !record.active || record.approval_binding.as_ref() != Some(&admitted.binding) => Some("extension_not_owner_activated_or_binding_changed".into()),
                Ok(_) if policy.capability_enabled.get("extension.external") != Some(&true) => Some("extension_capability_disabled_in_current_policy".into()),
                Ok(_) if policy.sandbox_scope == argentum_domain::SandboxScope::ReadOnly && !matches!(record.definition.transport, ExtensionTransport::Skills) => Some("executable_or_remote_extension_denied_by_readonly_policy".into()),
                Ok(_) if record.definition.image_scope.is_some() && selected.as_ref().is_none_or(|profile| profile.id != "chatgpt" || profile.kind != argentum_domain::ProviderKind::OpenAiCompatible || profile.endpoint != "https://api.openai.com/v1/") => Some("configured_route_has_no_supported_image_input_use_dom_only_or_current_approved_vision_route".into()),
                Ok(_) => None,
            };
            output.push(json!({"namespace":record.definition.namespace,"revision":record.revision,"saved_active":record.active,
                "definition_ready":issue.is_none(),"issue":issue,"dependency_leases":inspected.as_ref().ok().map(|admitted|admitted.file_leases.len()),
                "provider_contact":false,"process_started":false,"connection_and_account_entitlement":"not_proven_by_local_readiness",
                "credential_presence":"not_inspected_by_definition_readiness"}));
        }
        Ok(output)
    }
    pub async fn configure_extension(
        &self,
        definition: ExtensionDefinition,
    ) -> Result<ExtensionRecord, super::RuntimeError> {
        let _guard = self.authority_admission.lock().await;
        admit(&definition, self.workspace.root())
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        let previous = self
            .store
            .extension_record(self.project_id, &definition.namespace)?;
        self.extension_owner()
            .retire_namespace(&definition.namespace)
            .await?;
        let record = ExtensionRecord {
            definition,
            revision: Uuid::new_v4(),
            active: false,
            approval_binding: None,
        };
        self.store.save_extension_record(
            self.project_id,
            &record,
            previous.map(|record| record.revision),
        )?;
        Ok(record)
    }
    pub fn extension_activation_action(
        &self,
        namespace: &str,
    ) -> Result<argentum_security::LocalAuthorityAction, super::RuntimeError> {
        let record = self
            .store
            .extension_record(self.project_id, namespace)?
            .ok_or_else(|| ToolError::InvalidInput("extension_not_configured".into()))?;
        admit(&record.definition, self.workspace.root())
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        let current = self.execution_policy()?;
        if current.sandbox_scope == argentum_domain::SandboxScope::ReadOnly
            && !matches!(record.definition.transport, ExtensionTransport::Skills)
        {
            return Err(argentum_security::SecurityError::SandboxDenied(
                argentum_domain::Capability::UseExtensions,
            )
            .into());
        }
        let requested = self.with_harness(|harness| {
            harness.set_capability_enabled(&current, "extension.external", true)
        })?;
        let bytes = serde_json::to_vec(&(
            record,
            argentum_security::execution_policy_digest(&current),
            argentum_security::execution_policy_digest(&requested),
        ))
        .map_err(|_| ToolError::InvalidInput("invalid_extension_activation".into()))?;
        use sha2::Digest;
        Ok(argentum_security::LocalAuthorityAction::ActivateExtension {
            binding: Sha256Digest(format!("{:x}", sha2::Sha256::digest(bytes))),
        })
    }
    pub async fn activate_extension(
        &self,
        namespace: &str,
        ticket: argentum_security::LocalAuthorityTicket,
    ) -> Result<ExtensionRecord, super::RuntimeError> {
        let _guard = self.authority_admission.lock().await;
        let action = self.extension_activation_action(namespace)?;
        let proof = self.verify_local_authority_ticket(ticket, &action)?;
        let mut record = self
            .store
            .extension_record(self.project_id, namespace)?
            .ok_or_else(|| ToolError::InvalidInput("extension_not_configured".into()))?;
        let binding = approval_binding(&record.definition, self.workspace.root())
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        let current = self.execution_policy()?;
        let requested = self.with_harness(|harness| {
            harness.set_capability_enabled(&current, "extension.external", true)
        })?;
        proof.authorize(&action, &self.current_local_authority_generation()?)?;
        let previous = record.revision;
        record.revision = Uuid::new_v4();
        record.active = true;
        record.approval_binding = Some(binding);
        self.store
            .save_extension_activation(self.project_id, &record, previous, &requested)?;
        self.publish_workspace_state()?;
        Ok(record)
    }
    pub async fn disable_extension(
        &self,
        namespace: &str,
        remove: bool,
    ) -> Result<(), super::RuntimeError> {
        let _guard = self.authority_admission.lock().await;
        let mut record = self
            .store
            .extension_record(self.project_id, namespace)?
            .ok_or_else(|| ToolError::InvalidInput("extension_not_configured".into()))?;
        let previous = record.revision;
        record.active = false;
        record.approval_binding = None;
        record.revision = Uuid::new_v4();
        self.store
            .save_extension_record(self.project_id, &record, Some(previous))?;
        self.extension_owner().retire_namespace(namespace).await?;
        if remove {
            self.store
                .remove_extension_record(self.project_id, namespace, record.revision)?;
        }
        Ok(())
    }
}
