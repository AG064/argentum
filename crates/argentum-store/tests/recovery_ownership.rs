#![cfg(windows)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use argentum_domain::context_budget::{ModelBudget, RequestAccounting};
use argentum_domain::goal_accounting::{ChargeState, GoalAccounting, GoalBinding, ObservedUsage};
use argentum_domain::goal_supervision::{GoalInvocation, GoalInvocationState, SUPERVISION_VERSION};
use argentum_domain::recovery::{
    OwnershipOutcome, RecoveryAccountingState, RecoveryEffectState, RecoveryExecutionState,
    RecoveryRequestState, RecoveryRunKind, MAX_RECOVERY_RUNS,
};
use argentum_domain::{
    now, AppEvent, ApprovalGrantRecord, ApprovalRequest, Capability, FileChangeKind,
    FileChangeOrigin, FileSnapshotRef, Goal, GoalLifecycle, GrantIssuer, GrantScope, GrantSubject,
    ProviderKind, Sha256Digest, TaskLifecycle,
};
use argentum_store::{
    CommandClaimInput, CommandIdentityScope, CommandIdentityState, CommandOutcomeKind, EventScope,
    PreparedChangeOperation, Store, StoreError, COMMAND_IDENTITY_VERSION,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const HELPER_DATABASE: &str = "ARGENTUM_F08_HELPER_DATABASE";
const HELPER_WORKSPACE: &str = "ARGENTUM_F08_HELPER_WORKSPACE";
const HELPER_BOUNDARY: &str = "ARGENTUM_F08_HELPER_BOUNDARY";
const HELPER_EXTRA_RUNS: &str = "ARGENTUM_F08_HELPER_EXTRA_RUNS";
const HELPER_ACCOUNTING: &str = "ARGENTUM_F08_HELPER_ACCOUNTING";
const HELPER_TERMINAL_INVOCATION: &str = "ARGENTUM_F08_HELPER_TERMINAL_INVOCATION";
const HELPER_COMPLETED_EFFECT: &str = "ARGENTUM_F08_HELPER_COMPLETED_EFFECT";

fn run_id(value: u128) -> Uuid {
    Uuid::from_u128(0xf080_0000_0000_0000_0000_0000_0000_0000 | value)
}

fn accounting_goal(project_id: Uuid, session_id: Uuid, id: u128) -> Goal {
    Goal {
        id: run_id(id),
        project_id,
        session_id,
        objective: "Exercise interrupted accounting boundaries".into(),
        lifecycle: GoalLifecycle::Active,
        token_budget: Some(10_000),
        tool_budget: Some(8),
        time_budget_seconds: Some(3_600),
        tokens_used: 0,
        tools_used: 0,
        iteration: 0,
        next_action: "Explicit recovery review".into(),
        verification_history: Vec::new(),
        created_at: now(),
        updated_at: now(),
        accounting: Some(GoalAccounting::new(false)),
        definition_revision: run_id(id + 1),
        acceptance: None,
        owner_attestation: None,
        completion: None,
    }
}

fn accounting_estimate() -> RequestAccounting {
    RequestAccounting::from_wire_bytes(
        &ModelBudget {
            revision: run_id(900),
            profile_id: "synthetic".into(),
            provider_kind: ProviderKind::OpenAiCompatible,
            endpoint: "http://127.0.0.1:1/v1/".into(),
            model: "synthetic".into(),
            usable_context_tokens: 10_000,
            input_limit_tokens: None,
            maximum_output_tokens: 100,
            requested_output_tokens: 100,
            uncertainty_tokens: 128,
        },
        100,
        10,
        10,
    )
    .expect("accounting estimate")
}

fn digest_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest(format!("{:x}", Sha256::digest(bytes)))
}

fn append_run_event(
    store: &Store,
    workspace_key: &str,
    project_id: Uuid,
    session_id: Uuid,
    run_id: Uuid,
    event: &AppEvent,
) {
    store
        .append_event_scoped(
            &EventScope {
                workspace_key: workspace_key.into(),
                project_id,
                session_id: Some(session_id),
                run_id: Some(run_id),
            },
            event,
        )
        .expect("append run event");
}

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

fn spawn_owner(database: &Path, workspace: &Path, boundary: &Path) -> OwnedChild {
    spawn_owner_with_extra(database, workspace, boundary, 0)
}

fn spawn_accounting_owner(database: &Path, workspace: &Path, boundary: &Path) -> OwnedChild {
    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "owned_host_fixture_process", "--nocapture"])
        .env(HELPER_DATABASE, database)
        .env(HELPER_WORKSPACE, workspace)
        .env(HELPER_BOUNDARY, boundary)
        .env(HELPER_ACCOUNTING, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn accounting owner fixture");
    OwnedChild(child)
}

fn spawn_terminal_invocation_owner(
    database: &Path,
    workspace: &Path,
    boundary: &Path,
) -> OwnedChild {
    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "owned_host_fixture_process", "--nocapture"])
        .env(HELPER_DATABASE, database)
        .env(HELPER_WORKSPACE, workspace)
        .env(HELPER_BOUNDARY, boundary)
        .env(HELPER_TERMINAL_INVOCATION, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn terminal invocation owner fixture");
    OwnedChild(child)
}

fn spawn_completed_effect_owner(database: &Path, workspace: &Path, boundary: &Path) -> OwnedChild {
    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "owned_host_fixture_process", "--nocapture"])
        .env(HELPER_DATABASE, database)
        .env(HELPER_WORKSPACE, workspace)
        .env(HELPER_BOUNDARY, boundary)
        .env(HELPER_COMPLETED_EFFECT, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn completed effect owner fixture");
    OwnedChild(child)
}

fn spawn_owner_with_extra(
    database: &Path,
    workspace: &Path,
    boundary: &Path,
    extra_runs: usize,
) -> OwnedChild {
    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "owned_host_fixture_process", "--nocapture"])
        .env(HELPER_DATABASE, database)
        .env(HELPER_WORKSPACE, workspace)
        .env(HELPER_BOUNDARY, boundary)
        .env(HELPER_EXTRA_RUNS, extra_runs.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn owned host fixture");
    OwnedChild(child)
}

fn wait_for_boundary(path: &Path) -> (Uuid, Uuid) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(payload) = std::fs::read_to_string(path) {
            let mut lines = payload.lines();
            if let (Some(project_id), Some(session_id)) = (lines.next(), lines.next()) {
                return (
                    project_id.parse().expect("boundary project UUID"),
                    session_id.parse().expect("boundary session UUID"),
                );
            }
        }
        assert!(
            Instant::now() < deadline,
            "owned host did not publish its durable boundary"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn seed_accounting_boundaries(
    store: &Store,
    workspace_key: &str,
    project_id: Uuid,
    prepared_session_id: Uuid,
) {
    let prepared_run = run_id(20);
    let prepared_request = run_id(120);
    let prepared_goal = accounting_goal(project_id, prepared_session_id, 220);
    store.save_goal(&prepared_goal).expect("prepared goal");
    store
        .claim_run_ownership(
            prepared_run,
            project_id,
            prepared_session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("prepared accounting run claim");
    store
        .admit_goal_run(
            project_id,
            prepared_session_id,
            GoalBinding {
                goal_id: prepared_goal.id,
                revision: prepared_goal
                    .accounting
                    .as_ref()
                    .expect("accounting")
                    .revision,
            },
            prepared_run,
            false,
        )
        .expect("prepared accounting run");
    store
        .reserve_goal_request(
            prepared_run,
            prepared_request,
            "main_generation",
            &accounting_estimate(),
        )
        .expect("prepared accounting reservation");
    store
        .mark_run_request(
            prepared_run,
            prepared_request,
            "main_generation",
            RecoveryRequestState::PreparedNotDispatched,
        )
        .expect("prepared accounting boundary");
    append_run_event(
        store,
        workspace_key,
        project_id,
        prepared_session_id,
        prepared_run,
        &AppEvent::RunStatusChanged {
            run_id: prepared_run,
            lifecycle: TaskLifecycle::Running,
        },
    );

    let dispatched_session = store
        .create_session(project_id, "Dispatched accounting fixture")
        .expect("dispatched session");
    let dispatched_run = run_id(21);
    let dispatched_request = run_id(121);
    let dispatched_goal = accounting_goal(project_id, dispatched_session.id, 230);
    store.save_goal(&dispatched_goal).expect("dispatched goal");
    store
        .claim_run_ownership(
            dispatched_run,
            project_id,
            dispatched_session.id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("dispatched accounting run claim");
    store
        .admit_goal_run(
            project_id,
            dispatched_session.id,
            GoalBinding {
                goal_id: dispatched_goal.id,
                revision: dispatched_goal
                    .accounting
                    .as_ref()
                    .expect("accounting")
                    .revision,
            },
            dispatched_run,
            false,
        )
        .expect("dispatched accounting run");
    store
        .reserve_goal_request(
            dispatched_run,
            dispatched_request,
            "main_generation",
            &accounting_estimate(),
        )
        .expect("dispatched accounting reservation");
    store
        .mark_run_request(
            dispatched_run,
            dispatched_request,
            "main_generation",
            RecoveryRequestState::PreparedNotDispatched,
        )
        .and_then(|_| store.goal_request_dispatched(dispatched_request))
        .and_then(|_| {
            store.mark_run_request(
                dispatched_run,
                dispatched_request,
                "main_generation",
                RecoveryRequestState::Dispatched,
            )
        })
        .expect("dispatched accounting boundary");
    append_run_event(
        store,
        workspace_key,
        project_id,
        dispatched_session.id,
        dispatched_run,
        &AppEvent::RunStatusChanged {
            run_id: dispatched_run,
            lifecycle: TaskLifecycle::Running,
        },
    );

    let settled_session = store
        .create_session(project_id, "Settled accounting fixture")
        .expect("settled session");
    let settled_run = run_id(22);
    let settled_request = run_id(122);
    let settled_goal = accounting_goal(project_id, settled_session.id, 240);
    store.save_goal(&settled_goal).expect("settled goal");
    store
        .claim_run_ownership(
            settled_run,
            project_id,
            settled_session.id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("settled accounting run claim");
    store
        .admit_goal_run(
            project_id,
            settled_session.id,
            GoalBinding {
                goal_id: settled_goal.id,
                revision: settled_goal
                    .accounting
                    .as_ref()
                    .expect("accounting")
                    .revision,
            },
            settled_run,
            false,
        )
        .expect("settled accounting run");
    store
        .reserve_goal_request(
            settled_run,
            settled_request,
            "main_generation",
            &accounting_estimate(),
        )
        .expect("settled accounting reservation");
    store
        .mark_run_request(
            settled_run,
            settled_request,
            "main_generation",
            RecoveryRequestState::PreparedNotDispatched,
        )
        .and_then(|_| store.goal_request_dispatched(settled_request))
        .and_then(|_| {
            store.mark_run_request(
                settled_run,
                settled_request,
                "main_generation",
                RecoveryRequestState::Dispatched,
            )
        })
        .and_then(|_| {
            store.mark_run_request(
                settled_run,
                settled_request,
                "main_generation",
                RecoveryRequestState::ResponseReceived,
            )
        })
        .and_then(|_| {
            store
                .settle_goal_request(
                    settled_request,
                    Some(ObservedUsage {
                        input_tokens: Some(8),
                        output_tokens: Some(5),
                        total_tokens: 13,
                        cached_input_tokens: None,
                        reasoning_tokens: None,
                    }),
                )
                .map(|_| ())
        })
        .and_then(|_| {
            store.mark_run_request(
                settled_run,
                settled_request,
                "main_generation",
                RecoveryRequestState::Settled,
            )
        })
        .and_then(|_| {
            store
                .finish_goal_run(settled_run, 25, "complete")
                .map(|_| ())
        })
        .expect("settled accounting boundary");
    append_run_event(
        store,
        workspace_key,
        project_id,
        settled_session.id,
        settled_run,
        &AppEvent::RunStatusChanged {
            run_id: settled_run,
            lifecycle: TaskLifecycle::Complete,
        },
    );
}

fn seed_terminal_invocation_boundary(
    store: &Store,
    database: &Path,
    workspace_key: &str,
    project_id: Uuid,
    session_id: Uuid,
) {
    let goal = accounting_goal(project_id, session_id, 320);
    store.save_goal(&goal).expect("terminal invocation goal");
    let generation = Store::inspect_recovery(database)
        .expect("terminal invocation generation")
        .ownership
        .generation
        .expect("generation");
    let started_at = now();
    let invocation = GoalInvocation {
        version: SUPERVISION_VERSION,
        id: run_id(322),
        goal_id: goal.id,
        project_id,
        session_id,
        workspace_key: workspace_key.into(),
        owner_claim_id: run_id(321),
        owner_generation: Some(generation),
        owner_claim_retired: false,
        definition_revision: goal.definition_revision,
        criteria_version: 1,
        accounting_revision: goal.accounting.as_ref().expect("accounting").revision,
        iteration_limit: 1,
        elapsed_seconds: 60,
        started_at,
        deadline_at: started_at + time::Duration::seconds(60),
        state: GoalInvocationState::Verified,
        attempts: Vec::new(),
        evaluated_criteria: Vec::new(),
        progress: Vec::new(),
        stop_reason: Some("verified_against_declared_criteria".into()),
        delivery: None,
        finished_at: Some(started_at),
    };
    rusqlite::Connection::open(database)
        .expect("terminal invocation database")
        .execute(
            "INSERT INTO goal_invocations (
                 id, goal_id, project_id, session_id, state, started_at,
                 owner_generation, payload
             ) VALUES (?1, ?2, ?3, ?4, 'verified', ?5, ?6, ?7)",
            rusqlite::params![
                invocation.id.to_string(),
                invocation.goal_id.to_string(),
                invocation.project_id.to_string(),
                invocation.session_id.to_string(),
                invocation.started_at.unix_timestamp(),
                generation,
                serde_json::to_string(&invocation).expect("terminal invocation payload"),
            ],
        )
        .expect("terminal invocation boundary");
}

fn seed_completed_effect_boundary(
    store: &Store,
    workspace: &Path,
    workspace_key: &str,
    project_id: Uuid,
    session_id: Uuid,
) {
    let run = run_id(330);
    let call = run_id(331);
    let operation_id = run_id(332);
    let command = CommandClaimInput {
        project_id,
        request_id: "f09-post-effect-crash".into(),
        canonicalization_version: COMMAND_IDENTITY_VERSION,
        canonical_command: br#"{"kind":"submit_task","prompt":"effect boundary"}"#.to_vec(),
        command_kind: "submit_task".into(),
        scope: CommandIdentityScope {
            session_id: Some(session_id),
            run_id: Some(run),
            task_id: Some(run_id(333)),
            subject_id: None,
            ..CommandIdentityScope::default()
        },
    };
    store
        .claim_command_identity(&command)
        .expect("post-effect command claim");
    store
        .mark_command_executing(
            project_id,
            &command.request_id,
            command.canonicalization_version,
            &command.canonical_command,
        )
        .expect("post-effect command execution boundary");
    let after_bytes = b"committed effect survives process loss\n".to_vec();
    store
        .claim_run_ownership(
            run,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("completed effect run claim");
    append_run_event(
        store,
        workspace_key,
        project_id,
        session_id,
        run,
        &AppEvent::RunStatusChanged {
            run_id: run,
            lifecycle: TaskLifecycle::Running,
        },
    );
    store
        .prepare_change_operation(&PreparedChangeOperation {
            operation_id,
            workspace_key: workspace_key.into(),
            project_id,
            session_id,
            run_id: run,
            relative_path: "completed-effect.txt".into(),
            kind: FileChangeKind::Added,
            origin: FileChangeOrigin::ToolCall,
            before: None,
            after: Some(FileSnapshotRef {
                digest: digest_bytes(&after_bytes),
                size_bytes: after_bytes.len() as u64,
                retained: true,
            }),
            before_bytes: None,
            after_bytes: Some(after_bytes.clone()),
            additions: 1,
            removals: 0,
            line_count_availability: argentum_domain::LineCountAvailability::Unknown,
            restored_from: None,
        })
        .expect("prepare completed effect journal");
    store
        .mark_run_effect_admitted(run, call, "workspace_change", "workspace_change_journal")
        .expect("completed effect admission");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(workspace.join("completed-effect.txt"))
        .expect("create completed effect file");
    file.write_all(&after_bytes)
        .expect("write completed effect");
    file.sync_all().expect("sync completed effect");
    store
        .mark_change_operation_applied(operation_id)
        .and_then(|_| store.finalize_change_operation(operation_id).map(|_| ()))
        .expect("finalize completed effect journal");

    let claim_only = CommandClaimInput {
        project_id,
        request_id: "f09-claim-only-crash".into(),
        canonicalization_version: COMMAND_IDENTITY_VERSION,
        canonical_command: br#"{"kind":"pause_goal"}"#.to_vec(),
        command_kind: "pause_goal".into(),
        scope: CommandIdentityScope {
            session_id: Some(session_id),
            ..CommandIdentityScope::default()
        },
    };
    store
        .claim_command_identity(&claim_only)
        .expect("claim-only command boundary");

    let complete_run = run_id(334);
    let completed_command = CommandClaimInput {
        project_id,
        request_id: "f09-linked-complete-crash".into(),
        canonicalization_version: COMMAND_IDENTITY_VERSION,
        canonical_command: br#"{"kind":"submit_task","prompt":"completed lineage"}"#.to_vec(),
        command_kind: "submit_task".into(),
        scope: CommandIdentityScope {
            session_id: Some(session_id),
            run_id: Some(complete_run),
            task_id: Some(run_id(335)),
            subject_id: None,
            ..CommandIdentityScope::default()
        },
    };
    store
        .claim_command_identity(&completed_command)
        .and_then(|_| {
            store.mark_command_executing(
                project_id,
                &completed_command.request_id,
                completed_command.canonicalization_version,
                &completed_command.canonical_command,
            )
        })
        .expect("linked complete command boundary");
    store
        .claim_run_ownership(
            complete_run,
            project_id,
            session_id,
            Some(run_id(335)),
            None,
            RecoveryRunKind::Task,
        )
        .expect("linked complete run claim");
    append_run_event(
        store,
        workspace_key,
        project_id,
        session_id,
        complete_run,
        &AppEvent::RunStatusChanged {
            run_id: complete_run,
            lifecycle: TaskLifecycle::Complete,
        },
    );
    store
        .settle_active_run_ownership(complete_run, RecoveryExecutionState::Complete, "complete")
        .expect("linked complete run settlement");

    for (request_id, run, task, lifecycle, state, outcome) in [
        (
            "f09-linked-cancelled-crash",
            run_id(336),
            run_id(337),
            TaskLifecycle::Cancelled,
            RecoveryExecutionState::Cancelled,
            "cancelled",
        ),
        (
            "f09-linked-failed-crash",
            run_id(338),
            run_id(339),
            TaskLifecycle::Failed,
            RecoveryExecutionState::Failed,
            "failed",
        ),
    ] {
        let command = CommandClaimInput {
            project_id,
            request_id: request_id.into(),
            canonicalization_version: COMMAND_IDENTITY_VERSION,
            canonical_command: format!(r#"{{"kind":"submit_task","prompt":"{request_id}"}}"#)
                .into_bytes(),
            command_kind: "submit_task".into(),
            scope: CommandIdentityScope {
                session_id: Some(session_id),
                run_id: Some(run),
                task_id: Some(task),
                ..CommandIdentityScope::default()
            },
        };
        store
            .claim_command_identity(&command)
            .and_then(|_| {
                store.mark_command_executing(
                    project_id,
                    &command.request_id,
                    command.canonicalization_version,
                    &command.canonical_command,
                )
            })
            .expect("linked terminal command boundary");
        store
            .claim_run_ownership(
                run,
                project_id,
                session_id,
                Some(task),
                None,
                RecoveryRunKind::Task,
            )
            .expect("linked terminal run claim");
        append_run_event(
            store,
            workspace_key,
            project_id,
            session_id,
            run,
            &AppEvent::RunStatusChanged {
                run_id: run,
                lifecycle,
            },
        );
        store
            .settle_active_run_ownership(run, state, outcome)
            .expect("linked terminal run settlement");
    }
}

fn write_boundary_and_park(boundary: &Path, project_id: Uuid, session_id: Uuid) -> ! {
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(boundary)
        .expect("create boundary marker");
    writeln!(marker, "{project_id}\n{session_id}").expect("write boundary marker");
    marker.sync_all().expect("sync boundary marker");
    loop {
        std::thread::park();
    }
}

#[test]
fn owned_host_fixture_process() {
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
    let extra_runs = std::env::var(HELPER_EXTRA_RUNS)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    let (store, _owner) = Store::open_for_host(&database).expect("fixture host");
    let resolution = store
        .resolve_workspace(&workspace)
        .expect("fixture workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution
        .snapshot
        .active_session_id
        .expect("fixture session");

    if std::env::var_os(HELPER_ACCOUNTING).is_some() {
        seed_accounting_boundaries(&store, &resolution.workspace_key, project_id, session_id);
        write_boundary_and_park(&boundary, project_id, session_id);
    }
    if std::env::var_os(HELPER_TERMINAL_INVOCATION).is_some() {
        seed_terminal_invocation_boundary(
            &store,
            &database,
            &resolution.workspace_key,
            project_id,
            session_id,
        );
        write_boundary_and_park(&boundary, project_id, session_id);
    }
    if std::env::var_os(HELPER_COMPLETED_EFFECT).is_some() {
        seed_completed_effect_boundary(
            &store,
            &workspace,
            &resolution.workspace_key,
            project_id,
            session_id,
        );
        write_boundary_and_park(&boundary, project_id, session_id);
    }

    let prepared = run_id(1);
    store
        .claim_run_ownership(
            prepared,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("prepared run claim");
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        prepared,
        &AppEvent::RunStatusChanged {
            run_id: prepared,
            lifecycle: TaskLifecycle::Running,
        },
    );

    for offset in 0..extra_runs {
        let extra = run_id(1_000 + offset as u128);
        store
            .claim_run_ownership(
                extra,
                project_id,
                session_id,
                None,
                None,
                RecoveryRunKind::Task,
            )
            .expect("extra run claim");
        append_run_event(
            &store,
            &resolution.workspace_key,
            project_id,
            session_id,
            extra,
            &AppEvent::RunStatusChanged {
                run_id: extra,
                lifecycle: TaskLifecycle::Running,
            },
        );
    }
    store
        .mark_run_request(
            prepared,
            run_id(101),
            "main_generation",
            RecoveryRequestState::PreparedNotDispatched,
        )
        .expect("prepared request boundary");

    let partial = run_id(2);
    store
        .claim_run_ownership(
            partial,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Compaction,
        )
        .expect("partial run claim");
    store
        .mark_run_request(
            partial,
            run_id(102),
            "main_generation",
            RecoveryRequestState::PreparedNotDispatched,
        )
        .and_then(|_| {
            store.mark_run_request(
                partial,
                run_id(102),
                "main_generation",
                RecoveryRequestState::Dispatched,
            )
        })
        .and_then(|_| {
            store.mark_run_request(
                partial,
                run_id(102),
                "main_generation",
                RecoveryRequestState::PartialResponse,
            )
        })
        .expect("partial response boundary");

    let response = run_id(3);
    store
        .claim_run_ownership(
            response,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("response run claim");
    for state in [
        RecoveryRequestState::PreparedNotDispatched,
        RecoveryRequestState::Dispatched,
        RecoveryRequestState::ResponseReceived,
    ] {
        store
            .mark_run_request(response, run_id(103), "main_generation", state)
            .expect("response boundary");
    }

    let effect = run_id(4);
    let call_id = run_id(104);
    store
        .claim_run_ownership(
            effect,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("effect run claim");
    store
        .mark_run_effect_admitted(
            effect,
            call_id,
            "external_process",
            "windows_job_kill_on_close",
        )
        .expect("effect admission boundary");
    let approval_id = run_id(105);
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        effect,
        &AppEvent::ApprovalRequested(ApprovalRequest {
            id: approval_id,
            run_id: effect,
            tool_id: "execute_argv".into(),
            action: "Execute arguments".into(),
            target: "owned fixture".into(),
            binding: Sha256Digest("a".repeat(64)),
            reason: "exercise abandoned approval retirement".into(),
            capabilities: vec![Capability::ExecuteCommands, Capability::ExternalProcess],
            expires_at: None,
        }),
    );
    let claimed_approval_id = run_id(108);
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        effect,
        &AppEvent::ApprovalRequested(ApprovalRequest {
            id: claimed_approval_id,
            run_id: effect,
            tool_id: "execute_argv".into(),
            action: "Execute arguments".into(),
            target: "claimed owned fixture".into(),
            binding: Sha256Digest("b".repeat(64)),
            reason: "exercise claimed approval recovery".into(),
            capabilities: vec![Capability::ExecuteCommands, Capability::ExternalProcess],
            expires_at: None,
        }),
    );
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        effect,
        &AppEvent::ApprovalResolved {
            approval_id: claimed_approval_id,
            approved: true,
        },
    );
    store
        .save_approval_grant(
            project_id,
            &ApprovalGrantRecord {
                id: run_id(106),
                audit_id: run_id(107),
                capability: Capability::ExecuteCommands,
                scope: GrantScope::Run,
                subject: GrantSubject::Run { run_id: effect },
                created_at: now(),
                expires_at: None,
                issuer: GrantIssuer::Policy,
                reason: "fixture run authority".into(),
                revoked_at: None,
            },
        )
        .expect("run grant");

    let terminal = run_id(5);
    store
        .claim_run_ownership(
            terminal,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("terminal run claim");
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        terminal,
        &AppEvent::RunStatusChanged {
            run_id: terminal,
            lifecycle: TaskLifecycle::Complete,
        },
    );

    write_boundary_and_park(&boundary, project_id, session_id);
}

#[test]
fn live_owner_is_protected_and_killed_owner_reconciles_once_without_replay() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let boundary = fixture.path().join("owner-boundary.txt");
    let copy = fixture.path().join("copied-state.db");
    let mut owner = spawn_owner(&database, &workspace, &boundary);
    let (project_id, session_id) = wait_for_boundary(&boundary);

    let live = Store::inspect_recovery(&database).expect("live inspection");
    assert_eq!(live.ownership.outcome, OwnershipOutcome::Live);
    let live_generation = live.ownership.generation.expect("live generation");
    rusqlite::Connection::open(&database)
        .expect("process identity mismatch fixture")
        .execute(
            "UPDATE runtime_host_generations
             SET process_id = 4, process_start_key = '1'
             WHERE generation = ?1",
            [live_generation],
        )
        .expect("replace diagnostic process identity");
    let mismatched_process =
        Store::inspect_recovery(&database).expect("mismatched process inspection");
    assert_eq!(mismatched_process.ownership.process_id, Some(4));
    assert_eq!(mismatched_process.ownership.process_start_key, Some(1));
    assert_eq!(mismatched_process.ownership.outcome, OwnershipOutcome::Live);
    assert!(matches!(
        Store::open_for_host(&database),
        Err(StoreError::HostOwnershipLive)
    ));
    assert!(matches!(
        Store::reconcile_recovery(&database),
        Err(StoreError::HostOwnershipLive)
    ));
    let live_connection = rusqlite::Connection::open(&database).expect("live owner evidence");
    let live_resolved_approvals: usize = live_connection
        .query_row(
            "SELECT COUNT(*) FROM event_log WHERE event_type = 'approval_resolved'",
            [],
            |row| row.get(0),
        )
        .expect("live approval count");
    let live_revoked_grants: usize = live_connection
        .query_row(
            "SELECT COUNT(*) FROM approval_grants WHERE revoked_at IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("live grant count");
    assert_eq!(live_resolved_approvals, 1);
    assert_eq!(live_revoked_grants, 0);
    drop(live_connection);

    owner.terminate();
    rusqlite::Connection::open(&database)
        .expect("open abandoned database for coherent copy")
        .execute("VACUUM INTO ?1", [copy.to_string_lossy().as_ref()])
        .expect("copy abandoned database");
    let copied = Store::inspect_recovery(&copy).expect("copied inspection");
    assert_eq!(copied.ownership.outcome, OwnershipOutcome::OwnershipUnknown);
    assert!(matches!(
        Store::reconcile_recovery(&copy),
        Err(StoreError::HostOwnershipUnknown(
            "active_owner_belongs_to_a_different_physical_store_copy"
        ))
    ));

    let abandoned = Store::inspect_recovery(&database).expect("abandoned inspection");
    assert_eq!(
        abandoned.ownership.outcome,
        OwnershipOutcome::ProvenAbandoned
    );
    let settlement = Store::reconcile_recovery(&database).expect("reconcile abandoned owner");
    assert!(settlement.mutation_performed);
    assert!(!settlement.incomplete);
    assert_eq!(settlement.recovered_run_ids.len(), 4);
    let revision = settlement.recovery_revision.expect("recovery revision");

    let settled = Store::inspect_recovery(&database).expect("settled inspection");
    assert_eq!(settled.ownership.outcome, OwnershipOutcome::NoActiveOwner);
    let prepared = settled
        .runs
        .iter()
        .find(|run| run.run_id == run_id(1))
        .expect("prepared run");
    assert_eq!(
        prepared.execution_state,
        RecoveryExecutionState::Interrupted
    );
    assert_eq!(prepared.original_lifecycle, "running");
    assert_eq!(
        prepared.request_state,
        RecoveryRequestState::PreparedNotDispatched
    );
    assert_eq!(
        prepared.accounting_state,
        RecoveryAccountingState::NotAssociated
    );
    assert!(prepared.unresolved_effects.is_empty());
    assert!(prepared.admission_blocked);

    let partial = settled
        .runs
        .iter()
        .find(|run| run.run_id == run_id(2))
        .expect("partial run");
    assert_eq!(partial.kind, RecoveryRunKind::Compaction);
    assert!(partial
        .unresolved_effects
        .contains(&"provider_request_outcome_unknown".to_owned()));
    let response = settled
        .runs
        .iter()
        .find(|run| run.run_id == run_id(3))
        .expect("response run");
    assert_eq!(
        response.request_state,
        RecoveryRequestState::ResponseReceived
    );
    assert!(response.unresolved_effects.is_empty());
    let effect = settled
        .runs
        .iter()
        .find(|run| run.run_id == run_id(4))
        .expect("effect run");
    assert_eq!(effect.effect_state, RecoveryEffectState::Admitted);
    assert!(effect
        .unresolved_effects
        .contains(&"contained_effect_outcome_unknown".to_owned()));
    assert!(effect
        .unresolved_effects
        .contains(&"approval_claimed_without_settled_effect".to_owned()));
    assert!(effect.authority_invalidated);
    let terminal = settled
        .runs
        .iter()
        .find(|run| run.run_id == run_id(5))
        .expect("terminal run");
    assert_eq!(terminal.execution_state, RecoveryExecutionState::Complete);
    assert!(terminal.recovery_revision.is_none());

    let repeated = Store::reconcile_recovery(&database).expect("idempotent reconcile");
    assert!(!repeated.mutation_performed);
    assert!(repeated.recovered_run_ids.is_empty());
    let reader = Store::open(&database).expect("event reader");
    let events = reader.events_for_workspace(&workspace).expect("events");
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AppEvent::RunStatusChanged { run_id: id, lifecycle: TaskLifecycle::Interrupted } if *id == run_id(1)))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AppEvent::ApprovalResolved { approval_id, approved: false } if *approval_id == run_id(105)))
            .count(),
        1
    );
    let grant = reader
        .approval_grants(project_id, 8)
        .expect("grants")
        .into_iter()
        .find(|grant| grant.id == run_id(106))
        .expect("fixture grant");
    assert!(grant.revoked_at.is_some());
    drop(reader);

    let (unrelated_store, mut unrelated_owner) =
        Store::open_for_host(&database).expect("unrelated safe host");
    let unrelated_workspace_key = unrelated_store
        .resolve_workspace(&workspace)
        .expect("unrelated workspace")
        .workspace_key;
    let unrelated_session = unrelated_store
        .create_session(project_id, "Unrelated safe session")
        .expect("unrelated session");
    let unrelated_run = run_id(190);
    unrelated_store
        .claim_run_ownership(
            unrelated_run,
            project_id,
            unrelated_session.id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("unrelated session admission");
    append_run_event(
        &unrelated_store,
        &unrelated_workspace_key,
        project_id,
        unrelated_session.id,
        unrelated_run,
        &AppEvent::RunStatusChanged {
            run_id: unrelated_run,
            lifecycle: TaskLifecycle::Complete,
        },
    );
    unrelated_store
        .select_session(project_id, session_id)
        .expect("restore original session selection");
    unrelated_owner
        .retire()
        .expect("retire unrelated safe host");

    assert!(matches!(
        Store::review_recovery(&database, run_id(1), Uuid::new_v4()),
        Err(StoreError::RecoveryDecisionStale)
    ));
    for id in 1..=4 {
        let reviewed =
            Store::review_recovery(&database, run_id(id), revision).expect("owner review");
        assert!(!reviewed.admission_blocked);
        assert!(matches!(
            Store::review_recovery(&database, run_id(id), revision),
            Err(StoreError::RecoveryDecisionStale)
        ));
    }
    let reviewed_effect = Store::inspect_recovery(&database)
        .expect("reviewed inspection")
        .runs
        .into_iter()
        .find(|run| run.run_id == run_id(4))
        .expect("reviewed effect");
    assert!(reviewed_effect
        .unresolved_effects
        .contains(&"contained_effect_outcome_unknown".to_owned()));

    let (store, mut new_owner) = Store::open_for_host(&database).expect("new host");
    let reopened = store
        .resolve_workspace(&workspace)
        .expect("reopened workspace");
    assert_eq!(reopened.snapshot.project.id, project_id);
    assert_eq!(reopened.snapshot.active_session_id, Some(session_id));
    let fresh = run_id(200);
    store
        .claim_run_ownership(
            fresh,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("fresh explicit work");
    append_run_event(
        &store,
        &reopened.workspace_key,
        project_id,
        session_id,
        fresh,
        &AppEvent::RunStatusChanged {
            run_id: fresh,
            lifecycle: TaskLifecycle::Complete,
        },
    );
    new_owner.retire().expect("clean new host retirement");
}

#[test]
fn interrupted_accounting_releases_only_proven_undispatched_work() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let boundary = fixture.path().join("accounting-boundary.txt");
    let mut owner = spawn_accounting_owner(&database, &workspace, &boundary);
    let (project_id, _) = wait_for_boundary(&boundary);
    owner.terminate();

    let settlement = Store::reconcile_recovery(&database).expect("accounting recovery");
    assert!(settlement.mutation_performed);
    assert_eq!(settlement.recovered_run_ids.len(), 2);
    assert!(settlement.recovered_run_ids.contains(&run_id(20)));
    assert!(settlement.recovered_run_ids.contains(&run_id(21)));

    let recovery = Store::inspect_recovery(&database).expect("accounting inspection");
    let prepared_record = recovery
        .runs
        .iter()
        .find(|run| run.run_id == run_id(20))
        .expect("prepared recovery record");
    assert_eq!(
        prepared_record.request_state,
        RecoveryRequestState::PreparedNotDispatched
    );
    assert_eq!(
        prepared_record.accounting_state,
        RecoveryAccountingState::Unresolved
    );
    let dispatched_record = recovery
        .runs
        .iter()
        .find(|run| run.run_id == run_id(21))
        .expect("dispatched recovery record");
    assert_eq!(
        dispatched_record.request_state,
        RecoveryRequestState::Dispatched
    );
    assert_eq!(
        dispatched_record.accounting_state,
        RecoveryAccountingState::Unresolved
    );
    let settled_record = recovery
        .runs
        .iter()
        .find(|run| run.run_id == run_id(22))
        .expect("settled recovery record");
    assert_eq!(
        settled_record.execution_state,
        RecoveryExecutionState::Complete
    );
    assert_eq!(settled_record.request_state, RecoveryRequestState::Settled);
    assert_eq!(
        settled_record.accounting_state,
        RecoveryAccountingState::Settled
    );
    assert!(settled_record.recovery_revision.is_none());

    let reader = Store::open(&database).expect("accounting reader");
    let prepared = reader
        .inspect_goal_accounting(project_id, prepared_record.session_id, run_id(220))
        .expect("prepared accounting");
    assert_eq!(prepared.requests.len(), 1);
    assert_eq!(prepared.requests[0].state, ChargeState::NotDispatched);
    assert_eq!(prepared.requests[0].usage_quality, "proven_not_dispatched");
    let prepared_goal = reader
        .goal(project_id, prepared_record.session_id)
        .expect("prepared goal lookup")
        .expect("prepared goal");
    let prepared_counters = &prepared_goal
        .accounting
        .as_ref()
        .expect("accounting")
        .counters;
    assert_eq!(prepared_counters.outstanding_tokens, 0);
    assert_eq!(prepared_counters.unresolved_tokens, 0);

    let dispatched = reader
        .inspect_goal_accounting(project_id, dispatched_record.session_id, run_id(230))
        .expect("dispatched accounting");
    assert_eq!(dispatched.requests.len(), 1);
    assert_eq!(dispatched.requests[0].state, ChargeState::Unresolved);
    assert_eq!(
        dispatched.requests[0].usage_quality,
        "missing_final_usage_after_interruption"
    );
    let dispatched_goal = reader
        .goal(project_id, dispatched_record.session_id)
        .expect("dispatched goal lookup")
        .expect("dispatched goal");
    let dispatched_counters = &dispatched_goal
        .accounting
        .as_ref()
        .expect("accounting")
        .counters;
    assert_eq!(dispatched_counters.outstanding_tokens, 0);
    assert_eq!(
        dispatched_counters.unresolved_tokens,
        dispatched.requests[0].reserved_tokens
    );

    let settled = reader
        .inspect_goal_accounting(project_id, settled_record.session_id, run_id(240))
        .expect("settled accounting");
    assert_eq!(settled.requests.len(), 1);
    assert_eq!(settled.requests[0].state, ChargeState::Settled);
    assert_eq!(
        settled.requests[0]
            .observed
            .as_ref()
            .expect("settled usage")
            .total_tokens,
        13
    );
    let settled_goal = reader
        .goal(project_id, settled_record.session_id)
        .expect("settled goal lookup")
        .expect("settled goal");
    let settled_counters = &settled_goal
        .accounting
        .as_ref()
        .expect("accounting")
        .counters;
    assert_eq!(settled_counters.observed_tokens, 13);
    assert_eq!(settled_counters.outstanding_tokens, 0);
    assert_eq!(settled_counters.unresolved_tokens, 0);
    drop(reader);

    let repeated = Store::reconcile_recovery(&database).expect("idempotent accounting recovery");
    assert!(!repeated.mutation_performed);
    let reader = Store::open(&database).expect("reopened accounting reader");
    let repeated_settled = reader
        .inspect_goal_accounting(project_id, settled_record.session_id, run_id(240))
        .expect("repeated settled accounting");
    assert_eq!(repeated_settled, settled);
}

#[test]
fn recovery_retires_a_stale_terminal_invocation_without_rewriting_its_outcome() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let boundary = fixture.path().join("terminal-invocation-boundary.txt");
    let mut owner = spawn_terminal_invocation_owner(&database, &workspace, &boundary);
    wait_for_boundary(&boundary);
    owner.terminate();

    let settlement = Store::reconcile_recovery(&database).expect("terminal claim recovery");
    assert!(settlement.mutation_performed);
    assert!(settlement.recovered_run_ids.is_empty());
    assert_eq!(settlement.recovered_invocation_ids, vec![run_id(322)]);
    let reader = Store::open(&database).expect("terminal invocation reader");
    let invocation = reader
        .goal_invocation(run_id(322))
        .expect("terminal invocation lookup")
        .expect("terminal invocation");
    assert_eq!(invocation.state, GoalInvocationState::Verified);
    assert!(invocation.owner_claim_retired);
    assert_eq!(
        invocation.stop_reason.as_deref(),
        Some("verified_against_declared_criteria")
    );
    assert!(invocation.finished_at.is_some());
    drop(reader);

    let repeated = Store::reconcile_recovery(&database).expect("idempotent terminal recovery");
    assert!(!repeated.mutation_performed);
    assert!(repeated.recovered_invocation_ids.is_empty());
}

#[test]
fn completed_file_effect_remains_committed_when_its_run_terminal_is_lost() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let boundary = fixture.path().join("completed-effect-boundary.txt");
    let mut owner = spawn_completed_effect_owner(&database, &workspace, &boundary);
    let (project_id, _) = wait_for_boundary(&boundary);
    assert_eq!(
        std::fs::read_to_string(workspace.join("completed-effect.txt"))
            .expect("completed effect image"),
        "committed effect survives process loss\n"
    );
    owner.terminate();

    let settlement = Store::reconcile_recovery(&database).expect("completed effect recovery");
    assert_eq!(settlement.recovered_run_ids, vec![run_id(330)]);
    let revision = settlement.recovery_revision.expect("recovery revision");
    let recovered = Store::inspect_recovery(&database)
        .expect("completed effect inspection")
        .runs
        .into_iter()
        .find(|run| run.run_id == run_id(330))
        .expect("completed effect run");
    assert_eq!(
        recovered.execution_state,
        RecoveryExecutionState::Interrupted
    );
    assert_eq!(recovered.original_lifecycle, "running");
    assert_eq!(recovered.effect_state, RecoveryEffectState::Admitted);
    assert!(recovered.unresolved_effects.is_empty());
    assert_eq!(recovered.known_effects.len(), 1);
    assert_eq!(recovered.known_effects[0].kind, "change_operation");
    assert_eq!(recovered.known_effects[0].id, run_id(332));
    assert_eq!(recovered.known_effects[0].state, "finalized");
    let reader = Store::open(&database).expect("post-effect command reader");
    let command = reader
        .command_identity(project_id, "f09-post-effect-crash")
        .expect("post-effect command lookup")
        .expect("post-effect command identity");
    assert_eq!(command.state, CommandIdentityState::Interrupted);
    assert_eq!(command.outcome_kind, Some(CommandOutcomeKind::Interrupted));
    assert_eq!(command.recovery_revision, Some(revision));
    assert_eq!(command.scope.run_id, Some(run_id(330)));
    assert!(!command.result_available);
    let claim_only = reader
        .command_identity(project_id, "f09-claim-only-crash")
        .expect("claim-only command lookup")
        .expect("claim-only command identity");
    assert_eq!(claim_only.state, CommandIdentityState::Interrupted);
    assert_eq!(
        claim_only.outcome_kind,
        Some(CommandOutcomeKind::Interrupted)
    );
    assert_eq!(claim_only.recovery_revision, Some(revision));
    assert_eq!(
        claim_only.recovery_blocker.as_deref(),
        Some("owner_process_interrupted")
    );
    let completed = reader
        .command_identity(project_id, "f09-linked-complete-crash")
        .expect("linked complete command lookup")
        .expect("linked complete command identity");
    assert_eq!(completed.state, CommandIdentityState::Settled);
    assert_eq!(completed.outcome_kind, Some(CommandOutcomeKind::Success));
    assert_eq!(completed.recovery_revision, Some(revision));
    assert!(completed.result_available);
    assert!(completed.recovery_blocker.is_none());
    for (request_id, outcome) in [
        ("f09-linked-cancelled-crash", CommandOutcomeKind::Cancelled),
        ("f09-linked-failed-crash", CommandOutcomeKind::Failure),
    ] {
        let terminal = reader
            .command_identity(project_id, request_id)
            .expect("linked terminal command lookup")
            .expect("linked terminal command identity");
        assert_eq!(terminal.state, CommandIdentityState::Settled);
        assert_eq!(terminal.outcome_kind, Some(outcome));
        assert_eq!(terminal.recovery_revision, Some(revision));
        assert!(terminal.result_available);
        assert!(terminal.result_payload.is_none());
        assert!(terminal.recovery_blocker.is_none());
    }
    assert_eq!(
        std::fs::read_to_string(workspace.join("completed-effect.txt"))
            .expect("preserved completed effect"),
        "committed effect survives process loss\n"
    );
    let reviewed =
        Store::review_recovery(&database, run_id(330), revision).expect("completed effect review");
    assert!(!reviewed.admission_blocked);
    assert_eq!(
        std::fs::read_to_string(workspace.join("completed-effect.txt"))
            .expect("completed effect after review"),
        "committed effect survives process loss\n"
    );
}

#[test]
fn fenced_generation_rejects_late_boundaries_and_terminal_publication() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let (store, _owner) = Store::open_for_host(&database).expect("owned store");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution.snapshot.active_session_id.expect("session");
    let run = run_id(300);
    store
        .claim_run_ownership(
            run,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("run claim");
    let command = CommandClaimInput {
        project_id,
        request_id: "f09-stale-finalizer".into(),
        canonicalization_version: COMMAND_IDENTITY_VERSION,
        canonical_command: br#"{"kind":"submit_task","prompt":"stale"}"#.to_vec(),
        command_kind: "submit_task".into(),
        scope: CommandIdentityScope {
            session_id: Some(session_id),
            run_id: Some(run),
            task_id: Some(run_id(302)),
            subject_id: None,
            ..CommandIdentityScope::default()
        },
    };
    store
        .claim_command_identity(&command)
        .and_then(|_| {
            store.mark_command_executing(
                project_id,
                &command.request_id,
                command.canonicalization_version,
                &command.canonical_command,
            )
        })
        .expect("stale command boundary");
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        run,
        &AppEvent::RunStatusChanged {
            run_id: run,
            lifecycle: TaskLifecycle::Running,
        },
    );
    assert!(matches!(
        store.finish_run_ownership(run_id(301), RecoveryExecutionState::Complete, "complete"),
        Err(StoreError::HostGenerationFenced)
    ));

    let generation = Store::inspect_recovery(&database)
        .expect("live generation")
        .ownership
        .generation
        .expect("generation");
    rusqlite::Connection::open(&database)
        .expect("fence connection")
        .execute(
            "UPDATE runtime_host_generations SET state = 'recovered' WHERE generation = ?1",
            [generation],
        )
        .expect("fence old generation");

    assert!(matches!(
        store.ensure_run_generation(run),
        Err(StoreError::HostGenerationFenced)
    ));
    assert!(matches!(
        store.settle_command_identity(
            project_id,
            &command.request_id,
            command.canonicalization_version,
            &command.canonical_command,
            CommandOutcomeKind::Success,
            Some(r#"{"type":"command_completed"}"#),
        ),
        Err(StoreError::HostGenerationFenced)
    ));
    assert!(matches!(
        store.mark_run_request(
            run,
            run_id(302),
            "main_generation",
            RecoveryRequestState::PreparedNotDispatched
        ),
        Err(StoreError::HostGenerationFenced)
    ));
    assert!(matches!(
        store.append_event_scoped(
            &EventScope {
                workspace_key: resolution.workspace_key,
                project_id,
                session_id: Some(session_id),
                run_id: Some(run),
            },
            &AppEvent::RunStatusChanged {
                run_id: run,
                lifecycle: TaskLifecycle::Complete,
            }
        ),
        Err(StoreError::HostGenerationFenced)
    ));
    assert!(matches!(
        store.finish_run_ownership(run, RecoveryExecutionState::Complete, "complete"),
        Err(StoreError::HostGenerationFenced)
    ));
}

#[test]
fn reopened_host_rejects_terminal_publication_for_an_old_generation() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let (store, mut owner) = Store::open_for_host(&database).expect("owned store");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution.snapshot.active_session_id.expect("session");
    let run = run_id(350);
    store
        .claim_run_ownership(
            run,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("run claim");
    store
        .finish_run_ownership(run, RecoveryExecutionState::Complete, "complete")
        .expect("finish run");
    owner.retire().expect("retire first owner");
    drop(store);

    let (reopened, mut reopened_owner) = Store::open_for_host(&database).expect("reopened store");
    reopened
        .append_event_scoped(
            &EventScope {
                workspace_key: resolution.workspace_key.clone(),
                project_id,
                session_id: Some(session_id),
                run_id: Some(run),
            },
            &AppEvent::RunStatusChanged {
                run_id: run,
                lifecycle: TaskLifecycle::Reviewing,
            },
        )
        .expect("historical review event");
    assert!(matches!(
        reopened.append_event_scoped(
            &EventScope {
                workspace_key: resolution.workspace_key.clone(),
                project_id,
                session_id: Some(session_id),
                run_id: Some(run),
            },
            &AppEvent::RunStatusChanged {
                run_id: run,
                lifecycle: TaskLifecycle::Failed,
            }
        ),
        Err(StoreError::HostGenerationFenced)
    ));
    assert!(matches!(
        reopened.append_event_scoped(
            &EventScope {
                workspace_key: resolution.workspace_key,
                project_id,
                session_id: Some(session_id),
                run_id: Some(run),
            },
            &AppEvent::RunStatusChanged {
                run_id: run_id(352),
                lifecycle: TaskLifecycle::Reviewing,
            }
        ),
        Err(StoreError::InvalidRecord(_))
    ));
    let claim = Store::inspect_recovery(&database)
        .expect("old claim inspection")
        .runs
        .into_iter()
        .find(|candidate| candidate.run_id == run)
        .expect("old claim");
    assert_eq!(claim.execution_state, RecoveryExecutionState::Complete);
    reopened_owner.retire().expect("retire reopened owner");
}

#[test]
fn terminal_run_claim_does_not_regress_on_historical_review() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let (store, mut owner) = Store::open_for_host(&database).expect("owned store");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution.snapshot.active_session_id.expect("session");
    let run = run_id(351);
    store
        .claim_run_ownership(
            run,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("run claim");
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        run,
        &AppEvent::RunStatusChanged {
            run_id: run,
            lifecycle: TaskLifecycle::Complete,
        },
    );
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        run,
        &AppEvent::RunStatusChanged {
            run_id: run,
            lifecycle: TaskLifecycle::Reviewing,
        },
    );

    let claim = Store::inspect_recovery(&database)
        .expect("claim inspection")
        .runs
        .into_iter()
        .find(|candidate| candidate.run_id == run)
        .expect("run claim");
    assert_eq!(claim.execution_state, RecoveryExecutionState::Complete);
    assert_eq!(claim.original_lifecycle, "complete");
    owner.retire().expect("retire completed owner");
}

#[test]
fn request_recovery_identity_is_immutable_until_the_request_is_settled() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let (store, mut owner) = Store::open_for_host(&database).expect("owned store");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution.snapshot.active_session_id.expect("session");
    let run = run_id(340);
    let first_request = run_id(341);
    let second_request = run_id(342);
    store
        .claim_run_ownership(
            run,
            project_id,
            session_id,
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("run claim");
    store
        .mark_run_request(
            run,
            first_request,
            "main_generation",
            RecoveryRequestState::PreparedNotDispatched,
        )
        .expect("first request prepared");
    assert!(matches!(
        store.mark_run_request(
            run,
            first_request,
            "compaction",
            RecoveryRequestState::Dispatched,
        ),
        Err(StoreError::InvalidRecord(_))
    ));
    assert!(matches!(
        store.mark_run_request(
            run,
            second_request,
            "main_generation",
            RecoveryRequestState::PreparedNotDispatched,
        ),
        Err(StoreError::InvalidRecord(_))
    ));
    store
        .mark_run_request(
            run,
            first_request,
            "main_generation",
            RecoveryRequestState::Settled,
        )
        .expect("first request settled");
    store
        .mark_run_request(
            run,
            second_request,
            "main_generation",
            RecoveryRequestState::PreparedNotDispatched,
        )
        .expect("second request prepared after settlement");
    store
        .finish_run_ownership(run, RecoveryExecutionState::Complete, "complete")
        .expect("finish run");
    owner.retire().expect("retire owner");
}

#[test]
fn windows_mutex_owner_retires_on_its_acquisition_thread_after_host_move() {
    let fixture = tempfile::tempdir().expect("fixture");
    let database = fixture.path().join("state.db");
    let (store, owner) = Store::open_for_host(&database).expect("owned store");
    std::thread::spawn(move || {
        let mut owner = owner;
        owner.retire().expect("retire moved owner");
    })
    .join()
    .expect("owner retirement thread");
    drop(store);

    let (_reopened, mut reopened_owner) =
        Store::open_for_host(&database).expect("reopen after moved retirement");
    reopened_owner.retire().expect("retire reopened owner");
}

#[test]
fn recovery_batches_are_bounded_and_resume_until_the_generation_is_settled() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let boundary = fixture.path().join("owner-boundary.txt");
    let extra_runs = MAX_RECOVERY_RUNS + 1;
    let mut owner = spawn_owner_with_extra(&database, &workspace, &boundary, extra_runs);
    wait_for_boundary(&boundary);
    owner.terminate();

    let first = Store::reconcile_recovery(&database).expect("first bounded batch");
    assert!(first.mutation_performed);
    assert!(first.incomplete);
    assert_eq!(first.recovered_run_ids.len(), MAX_RECOVERY_RUNS);
    let first_revision = first.recovery_revision.expect("first recovery revision");

    let pending = Store::inspect_recovery(&database).expect("pending inspection");
    assert_eq!(
        pending.ownership.state,
        Some(argentum_domain::recovery::HostGenerationState::RecoveryPending)
    );

    let second = Store::reconcile_recovery(&database).expect("second bounded batch");
    assert!(second.mutation_performed);
    assert!(!second.incomplete);
    assert_eq!(second.recovered_run_ids.len(), 5);
    let second_revision = second.recovery_revision.expect("second recovery revision");
    assert_ne!(first_revision, second_revision);

    let all_recovered = first
        .recovered_run_ids
        .iter()
        .chain(&second.recovered_run_ids)
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(all_recovered.len(), MAX_RECOVERY_RUNS + 5);

    let final_pass = Store::reconcile_recovery(&database).expect("idempotent final pass");
    assert!(!final_pass.mutation_performed);
    assert!(!final_pass.incomplete);
}

#[test]
fn failed_reconciliation_rolls_back_and_concurrent_retries_have_one_winner() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let boundary = fixture.path().join("owner-boundary.txt");
    let mut owner = spawn_owner(&database, &workspace, &boundary);
    wait_for_boundary(&boundary);
    owner.terminate();

    let connection = rusqlite::Connection::open(&database).expect("open recovery fixture");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_recovery_generation_update
             BEFORE UPDATE OF state ON runtime_host_generations
             WHEN OLD.state IN ('active','recovery_pending')
               AND NEW.state IN ('recovery_pending','recovered')
             BEGIN
                 SELECT RAISE(ABORT, 'forced recovery generation failure');
             END;",
        )
        .expect("install failure trigger");
    drop(connection);

    assert!(matches!(
        Store::reconcile_recovery(&database),
        Err(StoreError::Database(_))
    ));
    let connection = rusqlite::Connection::open(&database).expect("inspect rollback");
    let active_runs: usize = connection
        .query_row(
            "SELECT COUNT(*) FROM runtime_run_ownership WHERE state = 'active'",
            [],
            |row| row.get(0),
        )
        .expect("active run count");
    let interrupted_runs: usize = connection
        .query_row(
            "SELECT COUNT(*) FROM runtime_run_ownership WHERE state = 'interrupted'",
            [],
            |row| row.get(0),
        )
        .expect("interrupted run count");
    let resolved_approvals: usize = connection
        .query_row(
            "SELECT COUNT(*) FROM event_log WHERE event_type = 'approval_resolved'",
            [],
            |row| row.get(0),
        )
        .expect("resolved approval count");
    let revoked_grants: usize = connection
        .query_row(
            "SELECT COUNT(*) FROM approval_grants WHERE revoked_at IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("revoked grant count");
    assert_eq!(active_runs, 4);
    assert_eq!(interrupted_runs, 0);
    assert_eq!(resolved_approvals, 1);
    assert_eq!(revoked_grants, 0);
    connection
        .execute_batch("DROP TRIGGER reject_recovery_generation_update;")
        .expect("remove failure trigger");
    drop(connection);

    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let database = database.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            Store::reconcile_recovery(database)
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().expect("recovery worker"))
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Ok(settlement) if settlement.mutation_performed))
            .count(),
        1
    );
    assert!(results
        .iter()
        .all(|result| matches!(result, Ok(_) | Err(StoreError::HostOwnershipLive))));

    let final_pass = Store::reconcile_recovery(&database).expect("final recovery state");
    assert!(!final_pass.mutation_performed);
    let inspection = Store::inspect_recovery(&database).expect("final inspection");
    assert_eq!(
        inspection.ownership.outcome,
        OwnershipOutcome::NoActiveOwner
    );
    assert_eq!(
        inspection
            .runs
            .iter()
            .filter(|run| run.execution_state == RecoveryExecutionState::Interrupted)
            .count(),
        4
    );
}

#[test]
fn legacy_nonterminal_runs_remain_ownership_unknown() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let store = Store::open(&database).expect("create current database");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution.snapshot.active_session_id.expect("session");
    let legacy_run = run_id(700);
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        legacy_run,
        &AppEvent::RunStatusChanged {
            run_id: legacy_run,
            lifecycle: TaskLifecycle::Running,
        },
    );
    drop(store);

    let connection = rusqlite::Connection::open(&database).expect("open legacy fixture");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE runtime_run_ownership;
             DROP TABLE runtime_host_generations;
             DROP TABLE runtime_store_identity;
             PRAGMA user_version = 16;",
        )
        .expect("downgrade fixture schema");
    drop(connection);

    let inspection = Store::inspect_recovery(&database).expect("legacy inspection");
    assert_eq!(
        inspection.ownership.outcome,
        OwnershipOutcome::OwnershipUnknown
    );
    assert!(inspection.runs.is_empty());
    assert_eq!(inspection.legacy_ambiguities.len(), 1);
    assert_eq!(inspection.legacy_ambiguities[0].run_id, legacy_run);
    assert!(!inspection.legacy_ambiguities_truncated);
    assert!(inspection.legacy_work_ambiguities.is_empty());
    assert!(matches!(
        Store::reconcile_recovery(&database),
        Err(StoreError::HostOwnershipUnknown(
            "legacy_schema_without_durable_owner_generation"
        ))
    ));
}

#[test]
fn terminal_legacy_history_does_not_hide_an_older_nonterminal_run() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let store = Store::open(&database).expect("create current database");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution.snapshot.active_session_id.expect("session");
    let legacy_run = run_id(720);
    append_run_event(
        &store,
        &resolution.workspace_key,
        project_id,
        session_id,
        legacy_run,
        &AppEvent::RunStatusChanged {
            run_id: legacy_run,
            lifecycle: TaskLifecycle::Running,
        },
    );
    for index in 0..33_u128 {
        let terminal_run = run_id(730 + index);
        append_run_event(
            &store,
            &resolution.workspace_key,
            project_id,
            session_id,
            terminal_run,
            &AppEvent::RunStatusChanged {
                run_id: terminal_run,
                lifecycle: TaskLifecycle::Complete,
            },
        );
    }
    drop(store);

    let connection = rusqlite::Connection::open(&database).expect("open legacy fixture");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE runtime_run_ownership;
             DROP TABLE runtime_host_generations;
             DROP TABLE runtime_store_identity;
             PRAGMA user_version = 16;",
        )
        .expect("downgrade fixture schema");
    drop(connection);

    let inspection = Store::inspect_recovery(&database).expect("legacy inspection");
    assert_eq!(inspection.legacy_ambiguities.len(), 1);
    assert_eq!(inspection.legacy_ambiguities[0].run_id, legacy_run);
    assert!(matches!(
        Store::open_for_host(&database),
        Err(StoreError::HostOwnershipUnknown(
            "legacy_nonterminal_work_has_no_lifetime_owner_evidence"
        ))
    ));
}

#[test]
fn malformed_latest_legacy_status_returns_a_controlled_error() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let store = Store::open(&database).expect("create current database");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let run = run_id(750);
    append_run_event(
        &store,
        &resolution.workspace_key,
        resolution.snapshot.project.id,
        resolution.snapshot.active_session_id.expect("session"),
        run,
        &AppEvent::RunStatusChanged {
            run_id: run,
            lifecycle: TaskLifecycle::Running,
        },
    );
    drop(store);
    let connection = rusqlite::Connection::open(&database).expect("legacy fixture");
    connection
        .execute(
            "UPDATE event_log SET payload = '{' WHERE run_id = ?1",
            [run.to_string()],
        )
        .expect("corrupt latest status");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE runtime_run_ownership;
             DROP TABLE runtime_host_generations;
             DROP TABLE runtime_store_identity;
             PRAGMA user_version = 16;",
        )
        .expect("downgrade fixture schema");
    drop(connection);

    assert!(matches!(
        Store::inspect_recovery(&database),
        Err(StoreError::Serialization(_))
    ));
    assert!(matches!(
        Store::open_for_host(&database),
        Err(StoreError::Serialization(_))
    ));
}

#[test]
fn terminal_legacy_status_with_a_mismatched_embedded_run_is_rejected() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let store = Store::open(&database).expect("create current database");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let row_run = run_id(755);
    let embedded_run = run_id(756);
    append_run_event(
        &store,
        &resolution.workspace_key,
        resolution.snapshot.project.id,
        resolution.snapshot.active_session_id.expect("session"),
        row_run,
        &AppEvent::RunStatusChanged {
            run_id: row_run,
            lifecycle: TaskLifecycle::Running,
        },
    );
    drop(store);
    let connection = rusqlite::Connection::open(&database).expect("legacy fixture");
    let payload = serde_json::to_string(&AppEvent::RunStatusChanged {
        run_id: embedded_run,
        lifecycle: TaskLifecycle::Complete,
    })
    .expect("terminal payload");
    connection
        .execute(
            "UPDATE event_log SET payload = ?1 WHERE run_id = ?2",
            rusqlite::params![payload, row_run.to_string()],
        )
        .expect("mismatch latest status");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE runtime_run_ownership;
             DROP TABLE runtime_host_generations;
             DROP TABLE runtime_store_identity;
             PRAGMA user_version = 16;",
        )
        .expect("downgrade fixture schema");
    drop(connection);

    assert!(matches!(
        Store::inspect_recovery(&database),
        Err(StoreError::InvalidRecord(_))
    ));
    assert!(matches!(
        Store::open_for_host(&database),
        Err(StoreError::InvalidRecord(_))
    ));
}

#[test]
fn legacy_active_invocation_without_status_event_blocks_new_host_generation() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let store = Store::open(&database).expect("create current database");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution.snapshot.active_session_id.expect("session");
    let goal = accounting_goal(project_id, session_id, 760);
    store.save_goal(&goal).expect("goal");
    let started_at = now();
    let invocation = GoalInvocation {
        version: SUPERVISION_VERSION,
        id: run_id(762),
        goal_id: goal.id,
        project_id,
        session_id,
        workspace_key: resolution.workspace_key,
        owner_claim_id: run_id(761),
        owner_generation: None,
        owner_claim_retired: false,
        definition_revision: goal.definition_revision,
        criteria_version: 1,
        accounting_revision: goal.accounting.as_ref().expect("accounting").revision,
        iteration_limit: 1,
        elapsed_seconds: 60,
        started_at,
        deadline_at: started_at + time::Duration::seconds(60),
        state: GoalInvocationState::Active,
        attempts: Vec::new(),
        evaluated_criteria: Vec::new(),
        progress: Vec::new(),
        stop_reason: None,
        finished_at: None,
        delivery: None,
    };
    rusqlite::Connection::open(&database)
        .expect("legacy invocation database")
        .execute(
            "INSERT INTO goal_invocations (
                 id, goal_id, project_id, session_id, state, started_at,
                 owner_generation, payload
             ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, NULL, ?6)",
            rusqlite::params![
                invocation.id.to_string(),
                invocation.goal_id.to_string(),
                invocation.project_id.to_string(),
                invocation.session_id.to_string(),
                invocation.started_at.unix_timestamp(),
                serde_json::to_string(&invocation).expect("invocation payload"),
            ],
        )
        .expect("legacy active invocation");
    drop(store);
    let connection = rusqlite::Connection::open(&database).expect("legacy fixture");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE runtime_run_ownership;
             DROP TABLE runtime_host_generations;
             DROP TABLE runtime_store_identity;
             PRAGMA user_version = 16;",
        )
        .expect("downgrade fixture schema");
    drop(connection);

    let inspection = Store::inspect_recovery(&database).expect("legacy inspection");
    assert_eq!(inspection.legacy_work_ambiguities.len(), 1);
    assert_eq!(inspection.legacy_work_ambiguities[0].id, invocation.id);
    assert_eq!(
        inspection.legacy_work_ambiguities[0].kind,
        "goal_invocation"
    );
    assert!(matches!(
        Store::open_for_host(&database),
        Err(StoreError::HostOwnershipUnknown(
            "legacy_nonterminal_work_has_no_lifetime_owner_evidence"
        ))
    ));
}

#[test]
fn legacy_unfinished_goal_run_without_status_event_blocks_new_host_generation() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let store = Store::open(&database).expect("create current database");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution.snapshot.active_session_id.expect("session");
    let goal = accounting_goal(project_id, session_id, 770);
    let run = run_id(772);
    store.save_goal(&goal).expect("goal");
    store
        .admit_goal_run(
            project_id,
            session_id,
            GoalBinding {
                goal_id: goal.id,
                revision: goal.accounting.as_ref().expect("accounting").revision,
            },
            run,
            false,
        )
        .expect("legacy unfinished goal run");
    drop(store);
    let connection = rusqlite::Connection::open(&database).expect("legacy fixture");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE runtime_run_ownership;
             DROP TABLE runtime_host_generations;
             DROP TABLE runtime_store_identity;
             PRAGMA user_version = 16;",
        )
        .expect("downgrade fixture schema");
    drop(connection);

    let inspection = Store::inspect_recovery(&database).expect("legacy inspection");
    assert_eq!(inspection.legacy_work_ambiguities.len(), 1);
    assert_eq!(inspection.legacy_work_ambiguities[0].id, run);
    assert_eq!(inspection.legacy_work_ambiguities[0].kind, "goal_run");
    assert!(matches!(
        Store::open_for_host(&database),
        Err(StoreError::HostOwnershipUnknown(
            "legacy_nonterminal_work_has_no_lifetime_owner_evidence"
        ))
    ));
}

#[test]
fn corrupt_recovery_payload_returns_a_controlled_serialization_error() {
    let fixture = tempfile::tempdir().expect("fixture");
    let database = fixture.path().join("state.db");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let (store, owner) = Store::open_for_host(&database).expect("host");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let run = run_id(800);
    store
        .claim_run_ownership(
            run,
            resolution.snapshot.project.id,
            resolution.snapshot.active_session_id.expect("session"),
            None,
            None,
            RecoveryRunKind::Task,
        )
        .expect("run claim");
    let connection = rusqlite::Connection::open(&database).expect("open corrupt fixture");
    connection
        .execute(
            "UPDATE runtime_run_ownership SET payload = '{' WHERE run_id = ?1",
            [run.to_string()],
        )
        .expect("corrupt recovery payload");
    drop(connection);

    assert!(matches!(
        Store::inspect_recovery(&database),
        Err(StoreError::Serialization(_))
    ));
    drop(store);
    drop(owner);
}
