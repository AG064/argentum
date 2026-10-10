use super::*;

async fn lookup_fixture() -> (tempfile::TempDir, TestRuntime, ToolRequest) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".agents")).unwrap();
    std::fs::write(root.path().join(".agents/verification.yaml"), "version: 1\nrunners:\n  - id: lookup-control\n    label: Lookup control\n    program: cargo\n    args: [test, --locked, --offline]\n    cwd: .\n    timeout_ms: 30000\n    output_limit: 65536\n").unwrap();
    let broker = CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap();
    let workspace = WorkspaceManager::new(broker);
    let runtime = test_runtime(
        Store::open_in_memory().unwrap(),
        ProviderRegistry::default(),
        ToolRegistry::with_builtins(workspace.clone()),
        workspace,
    );
    runtime
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    let at = now() - time::Duration::hours(1);
    for _ in 0..1617 {
        let record = synthetic_grant(
            GrantSubject::ToolCall {
                tool_call_id: Uuid::new_v4(),
            },
            at,
        );
        runtime
            .store
            .save_approval_grant(runtime.project_id, &record)
            .unwrap();
    }
    let request = ToolRequest {
        call_id: Uuid::new_v4(),
        run_id: Uuid::new_v4(),
        input: ToolInput::Verify {
            runners: vec![argentum_domain::VerificationRunnerId::Project(
                "lookup-control".into(),
            )],
        },
    };
    (root, runtime, request)
}

fn synthetic_grant(subject: GrantSubject, created_at: time::OffsetDateTime) -> ApprovalGrantRecord {
    ApprovalGrantRecord {
        id: Uuid::new_v4(),
        audit_id: Uuid::new_v4(),
        capability: Capability::ExternalProcess,
        scope: subject.scope(),
        subject,
        created_at,
        expires_at: None,
        issuer: GrantIssuer::User,
        reason: "Synthetic authority validity control".into(),
        revoked_at: None,
    }
}

fn exact_record(runtime: &RuntimeService, request: &ToolRequest) -> ApprovalGrantRecord {
    let ToolInput::Verify { runners } = &request.input else {
        panic!("verify request")
    };
    synthetic_grant(
        GrantSubject::Action {
            tool_id: "verify".into(),
            target: tool_target(&request.input),
            binding: runtime
                .tools
                .verification_binding(&runtime.workspace, runners)
                .unwrap(),
        },
        now() - time::Duration::minutes(1),
    )
}

fn prompts(runtime: &RuntimeService, request: &ToolRequest) -> bool {
    runtime
        .authorize_tool_request(request, &runtime.tool_descriptor(request).unwrap(), false)
        .unwrap()
        .prompt
}

#[tokio::test]
async fn grant_lookup_runtime_checks_revocation_expiry_future_and_definition_after_window() {
    let (root, runtime, request) = lookup_fixture().await;
    assert!(prompts(&runtime, &request));
    let valid = exact_record(&runtime, &request);
    runtime
        .store
        .save_approval_grant(runtime.project_id, &valid)
        .unwrap();
    assert!(!runtime
        .store
        .approval_grants(runtime.project_id, 1000)
        .unwrap()
        .iter()
        .any(|g| g.id == valid.id));
    assert!(!prompts(&runtime, &request));
    runtime
        .store
        .revoke_approval_grant(runtime.project_id, valid.id, now())
        .unwrap();
    assert!(prompts(&runtime, &request));
    let mut expired = exact_record(&runtime, &request);
    expired.created_at = now() - time::Duration::hours(2);
    expired.expires_at = Some(now() - time::Duration::hours(1));
    runtime
        .store
        .save_approval_grant(runtime.project_id, &expired)
        .unwrap();
    assert!(prompts(&runtime, &request));
    let mut future = exact_record(&runtime, &request);
    future.created_at = now() + time::Duration::hours(1);
    runtime
        .store
        .save_approval_grant(runtime.project_id, &future)
        .unwrap();
    assert!(prompts(&runtime, &request));
    let current = exact_record(&runtime, &request);
    runtime
        .store
        .save_approval_grant(runtime.project_id, &current)
        .unwrap();
    assert!(!prompts(&runtime, &request));
    let definition = root.path().join(".agents/verification.yaml");
    let text = std::fs::read_to_string(&definition).unwrap().replace(
        "args: [test, --locked, --offline]",
        "args: [test, --locked, --offline, --lib]",
    );
    std::fs::write(definition, text).unwrap();
    assert!(
        prompts(&runtime, &request),
        "changed runner reused its original exact grant"
    );
}

#[tokio::test]
async fn grant_lookup_does_not_override_current_policy_or_retired_authority() {
    let (root, runtime, request) = lookup_fixture().await;
    runtime
        .store
        .save_approval_grant(runtime.project_id, &exact_record(&runtime, &request))
        .unwrap();
    assert!(!prompts(&runtime, &request));
    runtime
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "observe".into(),
        })
        .await
        .unwrap();
    assert!(runtime.request_tool(request.clone()).await.is_err());
    assert!(!root.path().join("target").exists());
    runtime
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    // Retirement uses the production shutdown path, which closes admission
    // before retiring authority. A low-level test helper is not that path.
    runtime.shutdown().await.unwrap();
    assert!(runtime
        .runtime
        .dispatch(AppCommand::RequestTool { request })
        .await
        .is_err());
    assert!(!root.path().join("target").exists());
}
