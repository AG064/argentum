#![cfg(windows)]

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use argentum_domain::recovery::{
    OwnershipOutcome, RecoveryEffectState, RecoveryExecutionState, RecoveryRunKind,
};
use argentum_domain::{
    ApprovalMode, HarnessExecutionPolicy, SandboxScope, ToolDescriptor, ToolInput, ToolRequest,
};
use argentum_providers::ProviderRegistry;
use argentum_runtime::RuntimeService;
use argentum_security::{ApprovalPolicy, CapabilityBroker};
use argentum_store::{Store, StoreError};
use argentum_tools::{
    AgentTool, ToolContext, ToolError, ToolExecutionEvidence, ToolRegistry, ToolResult,
};
use argentum_workspaces::WorkspaceManager;
use async_trait::async_trait;
use uuid::Uuid;

const HELPER_DATABASE: &str = "ARGENTUM_F08_EFFECT_DATABASE";
const HELPER_WORKSPACE: &str = "ARGENTUM_F08_EFFECT_WORKSPACE";
const HELPER_BOUNDARY: &str = "ARGENTUM_F08_EFFECT_BOUNDARY";
const HELPER_COMPLETION: &str = "ARGENTUM_F08_EFFECT_COMPLETION";

struct OwnedChild(Child);

impl OwnedChild {
    fn terminate(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[derive(Clone)]
struct BlockingWriteTool {
    descriptor: ToolDescriptor,
    boundary: PathBuf,
}

#[async_trait]
impl AgentTool for BlockingWriteTool {
    fn descriptor(&self) -> ToolDescriptor {
        self.descriptor.clone()
    }

    async fn execute(
        &self,
        request: ToolRequest,
        mut context: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let patch = context
            .prepared_patch
            .take()
            .ok_or(ToolError::MissingPreparedMutation)?;
        let changed_paths = patch
            .mutations
            .iter()
            .map(|mutation| mutation.relative_path.clone())
            .collect::<Vec<_>>();
        context
            .workspace
            .apply_prepared_patch_with_scope_and_grant_cancellable(
                &patch,
                &context.approval,
                context.policy.sandbox_scope,
                || false,
            )?;
        let mut marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.boundary)
            .expect("create effect boundary marker");
        writeln!(marker, "{}\n{}", request.run_id, request.call_id)
            .expect("write effect boundary marker");
        marker.sync_all().expect("sync effect boundary marker");
        let _uncommitted_result = ToolResult {
            summary: "Applied workspace mutation".into(),
            output: String::new(),
            evidence: ToolExecutionEvidence {
                changed_paths,
                ..ToolExecutionEvidence::default()
            },
        };
        loop {
            std::thread::park();
        }
    }
}

fn workspace_runtime(store: Store, workspace_root: &Path, tools: ToolRegistry) -> RuntimeService {
    let broker = CapabilityBroker::new(workspace_root, ApprovalPolicy::default())
        .expect("capability broker");
    let workspace = WorkspaceManager::new(broker);
    RuntimeService::new(store, ProviderRegistry::default(), tools, workspace)
        .expect("workspace runtime")
}

#[tokio::test]
async fn owned_workspace_effect_fixture_process() {
    let (Ok(database), Ok(workspace), Ok(boundary)) = (
        std::env::var_os(HELPER_DATABASE).ok_or(()),
        std::env::var_os(HELPER_WORKSPACE).ok_or(()),
        std::env::var_os(HELPER_BOUNDARY).ok_or(()),
    ) else {
        return;
    };
    let database = PathBuf::from(database);
    let workspace = PathBuf::from(workspace);
    let boundary = PathBuf::from(boundary);
    let (store, _owner) = Store::open_for_host(&database).expect("fixture host");
    let resolution = store
        .resolve_workspace(&workspace)
        .expect("fixture workspace");
    let policy = HarnessExecutionPolicy {
        profile_id: "automatic".into(),
        approval_mode: ApprovalMode::Never,
        ..Default::default()
    };
    store
        .save_execution_policy(resolution.snapshot.project.id, &policy)
        .expect("automatic fixture policy");

    let workspace_manager = WorkspaceManager::new(
        CapabilityBroker::new(&workspace, ApprovalPolicy::default()).expect("fixture broker"),
    );
    let mut tools = ToolRegistry::with_builtins(workspace_manager.clone());
    let write_descriptor = tools
        .descriptors()
        .into_iter()
        .find(|descriptor| descriptor.id == "write_text")
        .expect("write descriptor");
    tools.register(BlockingWriteTool {
        descriptor: write_descriptor,
        boundary,
    });
    let runtime = RuntimeService::new(
        store.clone(),
        ProviderRegistry::default(),
        tools,
        workspace_manager,
    )
    .expect("fixture runtime");
    let run_id = Uuid::new_v4();
    store
        .claim_run_ownership(
            run_id,
            runtime.project_id(),
            runtime.session_id(),
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("effect run claim");
    runtime
        .request_tool(ToolRequest {
            call_id: Uuid::new_v4(),
            run_id,
            input: ToolInput::WriteText {
                path: "effect.txt".into(),
                content: "effect reached disk\n".into(),
            },
        })
        .await
        .expect("blocking write never returns before process loss");
}

#[test]
fn owned_process_payload_fixture() {
    let (Ok(boundary), Ok(completion)) = (
        std::env::var_os(HELPER_BOUNDARY).ok_or(()),
        std::env::var_os(HELPER_COMPLETION).ok_or(()),
    ) else {
        return;
    };
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(PathBuf::from(boundary))
        .expect("create process boundary marker");
    marker
        .write_all(b"started\n")
        .expect("write process boundary");
    marker.sync_all().expect("sync process boundary");
    std::thread::sleep(Duration::from_secs(5));
    let mut completed = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(PathBuf::from(completion))
        .expect("create process completion marker");
    completed
        .write_all(b"completed\n")
        .expect("write process completion");
    completed.sync_all().expect("sync process completion");
}

#[tokio::test]
async fn owned_process_effect_fixture_process() {
    let (Ok(database), Ok(workspace), Ok(boundary), Ok(completion)) = (
        std::env::var_os(HELPER_DATABASE).ok_or(()),
        std::env::var_os(HELPER_WORKSPACE).ok_or(()),
        std::env::var_os(HELPER_BOUNDARY).ok_or(()),
        std::env::var_os(HELPER_COMPLETION).ok_or(()),
    ) else {
        return;
    };
    let database = PathBuf::from(database);
    let workspace = PathBuf::from(workspace);
    let (store, _owner) = Store::open_for_host(&database).expect("fixture host");
    let resolution = store
        .resolve_workspace(&workspace)
        .expect("fixture workspace");
    let policy = HarnessExecutionPolicy {
        profile_id: "full-access".into(),
        sandbox_scope: SandboxScope::Unrestricted,
        approval_mode: ApprovalMode::Never,
        ..Default::default()
    };
    store
        .save_execution_policy(resolution.snapshot.project.id, &policy)
        .expect("automatic fixture policy");
    let broker =
        CapabilityBroker::new(&workspace, ApprovalPolicy::default()).expect("fixture broker");
    let workspace_manager = WorkspaceManager::new(broker);
    let runtime = RuntimeService::new(
        store.clone(),
        ProviderRegistry::default(),
        ToolRegistry::with_builtins(workspace_manager.clone()),
        workspace_manager,
    )
    .expect("fixture runtime");
    let run_id = Uuid::new_v4();
    store
        .claim_run_ownership(
            run_id,
            runtime.project_id(),
            runtime.session_id(),
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("process run claim");
    runtime
        .request_tool(ToolRequest {
            call_id: Uuid::new_v4(),
            run_id,
            input: ToolInput::ExecuteArgv {
                program: std::env::current_exe()
                    .expect("current test executable")
                    .to_string_lossy()
                    .into_owned(),
                args: vec![
                    "--exact".into(),
                    "owned_process_payload_fixture".into(),
                    "--nocapture".into(),
                ],
                cwd: None,
                timeout_ms: 15_000,
                environment: BTreeMap::from([
                    (
                        HELPER_BOUNDARY.into(),
                        PathBuf::from(boundary).to_string_lossy().into_owned(),
                    ),
                    (
                        HELPER_COMPLETION.into(),
                        PathBuf::from(completion).to_string_lossy().into_owned(),
                    ),
                ]),
            },
        })
        .await
        .expect("owned process never returns before process loss");
}

fn spawn_effect_owner(database: &Path, workspace: &Path, boundary: &Path) -> OwnedChild {
    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args([
            "--exact",
            "owned_workspace_effect_fixture_process",
            "--nocapture",
        ])
        .env(HELPER_DATABASE, database)
        .env(HELPER_WORKSPACE, workspace)
        .env(HELPER_BOUNDARY, boundary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn effect owner fixture");
    OwnedChild(child)
}

fn spawn_process_owner(
    database: &Path,
    workspace: &Path,
    boundary: &Path,
    completion: &Path,
) -> OwnedChild {
    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args([
            "--exact",
            "owned_process_effect_fixture_process",
            "--nocapture",
        ])
        .env(HELPER_DATABASE, database)
        .env(HELPER_WORKSPACE, workspace)
        .env(HELPER_BOUNDARY, boundary)
        .env(HELPER_COMPLETION, completion)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn process owner fixture");
    OwnedChild(child)
}

fn wait_for_boundary(path: &Path) -> (Uuid, Uuid) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(payload) = std::fs::read_to_string(path) {
            let mut lines = payload.lines();
            let run_id = lines
                .next()
                .expect("run boundary")
                .parse()
                .expect("run UUID");
            let call_id = lines
                .next()
                .expect("call boundary")
                .parse()
                .expect("call UUID");
            return (run_id, call_id);
        }
        assert!(
            Instant::now() < deadline,
            "effect owner did not publish its durable boundary"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[tokio::test]
async fn killed_journaled_effect_preserves_owner_edit_and_becomes_a_conflict() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let boundary = fixture.path().join("effect-boundary.txt");
    let mut owner = spawn_effect_owner(&database, &workspace, &boundary);
    let (run_id, call_id) = wait_for_boundary(&boundary);

    assert_eq!(
        std::fs::read_to_string(workspace.join("effect.txt")).expect("effect image"),
        "effect reached disk\n"
    );
    let live = Store::inspect_recovery(&database).expect("live inspection");
    assert_eq!(live.ownership.outcome, OwnershipOutcome::Live);
    let active = live
        .runs
        .iter()
        .find(|run| run.run_id == run_id)
        .expect("active effect run");
    assert_eq!(active.effect_call_id, Some(call_id));
    assert_eq!(active.effect_state, RecoveryEffectState::Admitted);
    let connection = rusqlite::Connection::open(&database).expect("inspect journal");
    let (operation_id, state): (String, String) = connection
        .query_row(
            "SELECT id, state FROM change_operations WHERE run_id = ?1",
            [run_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("prepared operation");
    assert_eq!(state, "prepared");
    drop(connection);

    owner.terminate();
    std::fs::write(
        workspace.join("effect.txt"),
        b"owner conflict after crash\n",
    )
    .expect("owner conflict");
    let settlement = Store::reconcile_recovery(&database).expect("owner reconciliation");
    let revision = settlement.recovery_revision.expect("recovery revision");
    assert_eq!(settlement.recovered_run_ids, vec![run_id]);
    let recovered = Store::inspect_recovery(&database)
        .expect("recovered inspection")
        .runs
        .into_iter()
        .find(|run| run.run_id == run_id)
        .expect("recovered effect run");
    assert_eq!(
        recovered.execution_state,
        RecoveryExecutionState::Interrupted
    );
    assert!(recovered
        .unresolved_effects
        .contains(&"prepared_file_change_requires_recovery".to_owned()));
    assert!(!recovered
        .unresolved_effects
        .contains(&"uncontained_process_may_still_be_running".to_owned()));
    assert_eq!(
        std::fs::read_to_string(workspace.join("effect.txt")).expect("preserved conflict"),
        "owner conflict after crash\n"
    );
    Store::review_recovery(&database, run_id, revision).expect("exact owner review");

    let (store, mut new_owner) = Store::open_for_host(&database).expect("reopened host");
    let runtime = workspace_runtime(
        store.clone(),
        &workspace,
        ToolRegistry::with_builtins(WorkspaceManager::new(
            CapabilityBroker::new(&workspace, ApprovalPolicy::default()).expect("reopened broker"),
        )),
    );
    let operation_id = Uuid::parse_str(&operation_id).expect("operation UUID");
    let operation = store
        .change_operation(runtime.project_id(), operation_id)
        .expect("change lookup")
        .expect("change operation");
    assert_eq!(
        operation.state,
        argentum_domain::ChangeRecoveryState::Conflicted
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("effect.txt")).expect("conflict after reopen"),
        "owner conflict after crash\n"
    );

    let fresh = Uuid::new_v4();
    store
        .claim_run_ownership(
            fresh,
            runtime.project_id(),
            runtime.session_id(),
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("fresh unrelated run");
    runtime
        .request_tool(ToolRequest {
            call_id: Uuid::new_v4(),
            run_id: fresh,
            input: ToolInput::WriteText {
                path: "unrelated.txt".into(),
                content: "new explicit work\n".into(),
            },
        })
        .await
        .expect("unrelated safe write");
    store
        .finish_run_ownership(fresh, RecoveryExecutionState::Complete, "complete")
        .expect("finish unrelated run");
    assert_eq!(
        std::fs::read_to_string(workspace.join("unrelated.txt")).expect("unrelated write"),
        "new explicit work\n"
    );
    drop(runtime);
    new_owner.retire().expect("clean reopened host retirement");
    assert!(matches!(
        store.ensure_run_generation(run_id),
        Err(StoreError::HostGenerationFenced)
    ));
}

#[test]
fn killed_owned_process_is_contained_without_claiming_its_outcome_known() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let boundary = fixture.path().join("process-boundary.txt");
    let completion = fixture.path().join("process-completion.txt");
    let mut owner = spawn_process_owner(&database, &workspace, &boundary, &completion);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !boundary.is_file() {
        assert!(
            Instant::now() < deadline,
            "owned process did not publish its durable boundary"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let live = Store::inspect_recovery(&database).expect("live process inspection");
    assert_eq!(live.ownership.outcome, OwnershipOutcome::Live);
    let active = live
        .runs
        .iter()
        .find(|run| {
            run.execution_state == RecoveryExecutionState::Active
                && run.effect_state == RecoveryEffectState::Admitted
                && run.effect_kind == "external_process"
        })
        .expect("active process run");
    assert_eq!(active.effect_containment, "windows_job_kill_on_close");
    let run_id = active.run_id;

    owner.terminate();
    std::thread::sleep(Duration::from_secs(6));
    assert!(
        !completion.exists(),
        "the owned child survived loss of its Windows job owner"
    );
    let settlement = Store::reconcile_recovery(&database).expect("process reconciliation");
    let revision = settlement
        .recovery_revision
        .expect("process recovery revision");
    assert_eq!(settlement.recovered_run_ids, vec![run_id]);
    let recovered = Store::inspect_recovery(&database)
        .expect("recovered process inspection")
        .runs
        .into_iter()
        .find(|run| run.run_id == run_id)
        .expect("recovered process run");
    assert_eq!(
        recovered.execution_state,
        RecoveryExecutionState::Interrupted
    );
    assert!(recovered
        .unresolved_effects
        .contains(&"contained_effect_outcome_unknown".to_owned()));
    let reviewed =
        Store::review_recovery(&database, run_id, revision).expect("exact process owner review");
    assert!(!reviewed.admission_blocked);
    assert!(reviewed
        .unresolved_effects
        .contains(&"contained_effect_outcome_unknown".to_owned()));
}
