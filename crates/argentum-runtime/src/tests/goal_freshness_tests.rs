use super::*;
use argentum_domain::{VerificationRunnerId, VerificationState};
use argentum_tools::{AgentTool, ToolContext, ToolError, ToolResult};

// A test-only wrapper around the unchanged built-in registry. The notification
// holds a real runner result before the runtime consumes and persists it.
struct SettledVerifierGate {
    inner: ToolRegistry,
    settled: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl AgentTool for SettledVerifierGate {
    fn descriptor(&self) -> argentum_domain::ToolDescriptor {
        self.inner
            .descriptor_for_input(&ToolInput::Verify {
                runners: vec![VerificationRunnerId::CargoFmtCheck],
            })
            .expect("built-in verifier descriptor")
    }

    async fn execute(
        &self,
        request: ToolRequest,
        context: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let result = self.inner.execute(request, context).await?;
        assert_eq!(result.evidence.verification.len(), 1);
        self.settled.notify_one();
        self.release.notified().await;
        Ok(result)
    }
}

async fn check_settled_verifier_boundary(formatted: bool, cancel: bool) {
    let directory = tempfile::tempdir().expect("workspace");
    write_format_fixture(directory.path(), formatted);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut providers = ProviderRegistry::default();
    providers.register(CountingAnswerProvider {
        id: "settled-verifier-answer",
        calls: Arc::clone(&calls),
    });
    let workspace = WorkspaceManager::new(
        CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).expect("broker"),
    );
    let inner = ToolRegistry::with_builtins(workspace.clone());
    let settled = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut tools = inner.clone();
    tools.register(SettledVerifierGate {
        inner,
        settled: Arc::clone(&settled),
        release: Arc::clone(&release),
    });
    let runtime = test_runtime(
        Store::open_in_memory().expect("store"),
        providers,
        tools,
        workspace,
    );
    let goal = configure_format_supervisor(&runtime, "settled-verifier-answer", true).await;
    let worker_runtime = runtime.runtime.clone();
    let worker = tokio::spawn(async move {
        worker_runtime
            .supervise_goal(argentum_domain::goal_supervision::SupervisionRequest {
                goal_id: goal.id,
                iteration_limit: 3,
                elapsed_seconds: 120,
                pause_on_session_switch: true,
            })
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(20), settled.notified())
        .await
        .expect("real verifier settled");
    let invocation = runtime
        .inspect_goal_supervision(goal.id)
        .expect("active inspection")
        .invocation
        .expect("active invocation");
    assert_eq!(invocation.state, GoalInvocationState::Active);
    assert!(runtime.goal().unwrap().unwrap().completion.is_none());
    if cancel {
        runtime
            .cancel_goal_invocation(invocation.id)
            .expect("owner cancellation");
    } else {
        std::fs::write(
            directory.path().join("src/lib.rs"),
            "pub fn owner_changed_after_verification()->bool{true}\n",
        )
        .expect("synchronized owner source edit");
    }
    release.notify_one();
    let status = tokio::time::timeout(std::time::Duration::from_secs(20), worker)
        .await
        .expect("supervisor settled without watchdog termination")
        .expect("supervisor join")
        .expect("bounded outcome");
    assert_eq!(status.attempts.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let stored = runtime.goal().expect("goal").expect("stored goal");
    assert!(stored.completion.is_none());
    let accounting = stored.accounting.expect("accounting");
    assert_eq!(accounting.counters.iterations, 1);
    assert_eq!(accounting.counters.request_attempts, 1);
    assert_eq!(accounting.counters.tool_admissions, 1);
    if cancel {
        assert_eq!(status.state, GoalInvocationState::Cancelled);
    } else {
        assert_eq!(status.state, GoalInvocationState::ReviewRequired);
        assert_eq!(status.stop_reason.as_deref(), Some("verification_stale"));
        let evidence = runtime
            .store
            .latest_verification_evidence(runtime.project_id, 8)
            .expect("original typed receipt");
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            evidence[0].state,
            if formatted {
                VerificationState::Passed
            } else {
                VerificationState::Failed
            }
        );
        assert_eq!(evidence[0].input_before, evidence[0].input_after);
        assert!(
            !runtime
                .inspect_goal_supervision(goal.id)
                .expect("historical inspection")
                .completion_current
        );
    }
}

#[tokio::test]
async fn source_edit_after_real_pass_blocks_completion_publication() {
    check_settled_verifier_boundary(true, false).await;
}

#[tokio::test]
async fn source_edit_after_real_failure_blocks_the_next_attempt() {
    check_settled_verifier_boundary(false, false).await;
}

#[tokio::test]
async fn owner_stop_after_real_verifier_pass_blocks_completion() {
    check_settled_verifier_boundary(true, true).await;
}
