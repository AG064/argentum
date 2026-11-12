use super::*;
use argentum_domain::text_contract::{TextReadResult, MAX_TEXT_RESULT_BYTES};
use argentum_domain::SandboxScope;
use argentum_security::ApprovalGrant;
use argentum_tools::{ToolContext, ToolExecutionEvidence};
use argentum_workspaces::WorkspaceTextSearchOptions;

#[tokio::test]
async fn short_json_escapes_use_existing_preview_budget_through_the_runtime_limiter() {
    use sha2::{Digest, Sha256};

    let directory = tempfile::tempdir().unwrap();
    let workspace = WorkspaceManager::new(
        CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap(),
    );
    let tools = ToolRegistry::with_builtins(workspace.clone());
    for unit in [
        "\u{0008}",
        "\u{000c}",
        "\n",
        "\r",
        "\t",
        "\u{0001}",
        "\"\\",
        "é中\u{10400}",
        "\t\ré中\\\n",
    ] {
        let source = unit.repeat(5000);
        let path = directory.path().join("preview.txt");
        std::fs::write(&path, source.as_bytes()).unwrap();
        let expected_digest = Sha256Digest(format!("{:x}", Sha256::digest(source.as_bytes())));
        let mut remaining = 8192 - 1024;
        let mut expected_end = 0;
        for character in source.chars() {
            let cost = serde_json::to_vec(&character.to_string()).unwrap().len() - 2;
            if cost > remaining {
                break;
            }
            remaining -= cost;
            expected_end += character.len_utf8();
        }
        let mut reconstructed = String::new();
        let mut next_start = 0;
        let mut reads = 0;
        loop {
            let result = tools
                .execute(
                    ToolRequest {
                        call_id: Uuid::new_v4(),
                        run_id: Uuid::new_v4(),
                        input: ToolInput::ReadText {
                            path: "preview.txt".into(),
                            start_byte: Some(next_start),
                            end_byte: (reads > 0).then_some(source.len() as u64),
                            start_line: None,
                            line_count: None,
                            expected_sha256: (reads > 0).then_some(expected_digest.clone()),
                        },
                    },
                    ToolContext::new(
                        workspace.clone(),
                        ApprovalGrant::default(),
                        HarnessExecutionPolicy::default(),
                        CancellationToken::new(),
                    ),
                )
                .await
                .unwrap();
            let original_output = result.output.clone();
            let output = bounded_tool_result(result);
            assert_eq!(output, original_output);
            assert!(
                output.len()
                    <= if reads == 0 {
                        8192
                    } else {
                        MAX_TEXT_RESULT_BYTES
                    }
            );
            let read: TextReadResult = serde_json::from_str(&output).unwrap();
            assert_eq!(read.sha256, expected_digest);
            assert_eq!(read.requested_end_byte, source.len() as u64);
            assert_eq!(read.returned_start_byte, next_start);
            assert!(source.is_char_boundary(read.returned_end_byte as usize));
            assert_eq!(
                read.content,
                source[next_start as usize..read.returned_end_byte as usize]
            );
            if reads == 0 {
                assert_eq!(read.returned_end_byte as usize, expected_end);
                assert!(!read.range_complete && read.file_has_more && !read.is_complete_file);
                assert!(serde_json::to_vec(&read.content).unwrap().len() - 2 <= 8192 - 1024);
            }
            reconstructed.push_str(&read.content);
            reads += 1;
            assert!(reads <= 8, "bounded continuations must make progress");
            if !read.file_has_more {
                assert!(read.range_complete);
                assert_eq!(read.next_start_byte, None);
                assert_eq!(read.returned_end_byte, source.len() as u64);
                break;
            }
            assert!(read.returned_end_byte > next_start);
            assert_eq!(read.next_start_byte, Some(read.returned_end_byte));
            next_start = read.next_start_byte.unwrap();
        }
        assert_eq!(reconstructed, source);
        assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    }
}

#[test]
fn fresh_restore_counts_and_provenance_use_the_same_admitted_images() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = WorkspaceManager::new(
        CapabilityBroker::new(dir.path(), ApprovalPolicy::default()).unwrap(),
    );
    let runtime = test_runtime(
        Store::open_in_memory().unwrap(),
        ProviderRegistry::default(),
        ToolRegistry::with_builtins(workspace.clone()),
        workspace,
    );
    std::fs::write(
        dir.path().join("large.txt"),
        vec![b'a'; MAX_SNAPSHOT_BYTES + 1],
    )
    .unwrap();
    let unretained = runtime
        .prepare_restore_binding("large.txt", Some(b"small\n"), None)
        .unwrap();
    assert_eq!((unretained.additions, unretained.removals), (0, 0));
    assert_eq!(
        unretained.line_count_availability,
        argentum_domain::LineCountAvailability::Unavailable
    );
    std::fs::write(dir.path().join("binary.txt"), b"valid\0utf8\n").unwrap();
    let binary = runtime
        .prepare_restore_binding("binary.txt", Some(b"text\n"), None)
        .unwrap();
    assert_eq!((binary.additions, binary.removals), (0, 0));
    assert_eq!(
        binary.line_count_availability,
        argentum_domain::LineCountAvailability::Unavailable
    );
    let added_empty = runtime
        .prepare_restore_binding("new.txt", Some(b""), None)
        .unwrap();
    assert_eq!((added_empty.additions, added_empty.removals), (0, 0));
    assert_eq!(
        added_empty.line_count_availability,
        argentum_domain::LineCountAvailability::Computed
    );
    std::fs::write(dir.path().join("text.txt"), b"old\n").unwrap();
    let known = runtime
        .prepare_restore_binding("text.txt", Some(b"new\n"), None)
        .unwrap();
    assert_eq!((known.additions, known.removals), (1, 1));
    assert_eq!(
        known.line_count_availability,
        argentum_domain::LineCountAvailability::Computed
    );
}

#[tokio::test]
async fn text_result_metadata_survives_final_model_limiter_at_all_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let broker = CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap();
    let workspace = WorkspaceManager::new(broker);
    let tools = ToolRegistry::with_builtins(workspace.clone());
    for size in [0, 65535, 65536, 65537, 131071, 131072, 131073, 350_000] {
        for unit in ["x", "\"\\\t\r\n", "é中\u{10400}"] {
            let original = unit.repeat(size / unit.len());
            std::fs::write(directory.path().join("text.txt"), &original).unwrap();
            let result = tools
                .execute(
                    ToolRequest {
                        call_id: Uuid::new_v4(),
                        run_id: Uuid::new_v4(),
                        input: ToolInput::ReadText {
                            path: "text.txt".into(),
                            start_byte: Some(0),
                            end_byte: None,
                            start_line: None,
                            line_count: None,
                            expected_sha256: None,
                        },
                    },
                    ToolContext::new(
                        workspace.clone(),
                        ApprovalGrant::default(),
                        HarnessExecutionPolicy::default(),
                        CancellationToken::new(),
                    ),
                )
                .await
                .unwrap();
            let original_output = result.output.clone();
            let wire = bounded_tool_result(result);
            assert_eq!(wire, original_output);
            assert!(wire.len() <= MAX_TEXT_RESULT_BYTES);
            let read: TextReadResult = serde_json::from_str(&wire).unwrap();
            assert_eq!(read.content, original[..read.returned_end_byte as usize]);
            assert_eq!(read.is_complete_file, read.content.len() == original.len());
            assert_eq!(read.range_complete, read.is_complete_file);
            assert_eq!(read.file_has_more, !read.is_complete_file);
            assert_eq!(
                read.next_start_byte,
                read.file_has_more.then_some(read.returned_end_byte)
            );
        }
    }
    let overflow = bounded_tool_result(ToolResult {
        summary: "read".into(),
        output: "x".repeat(MAX_TEXT_RESULT_BYTES + 1),
        evidence: ToolExecutionEvidence::default(),
    });
    assert!(overflow.contains("no content returned"));
    assert!(!overflow.contains("xxxx"));
}

fn text_fixture() -> (tempfile::TempDir, TestRuntime, ToolInput) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("text.txt"), "before\r\nunchanged\r\n").unwrap();
    let workspace = WorkspaceManager::new(
        CapabilityBroker::new(dir.path(), ApprovalPolicy::default()).unwrap(),
    );
    let digest = workspace
        .read_text_range_with_scope("text.txt", 0, None, None, SandboxScope::WorkspaceWrite)
        .unwrap()
        .sha256;
    let runtime = test_runtime(
        Store::open_in_memory().unwrap(),
        ProviderRegistry::default(),
        ToolRegistry::with_builtins(workspace.clone()),
        workspace,
    );
    let input = ToolInput::EditText {
        path: "text.txt".into(),
        expected_sha256: digest,
        old_text: Some("before".into()),
        new_text: Some("after".into()),
        newline_mode: argentum_domain::text_contract::TextNewlineMode::Exact,
        start_line: None,
        line_count: None,
        replacement_lines: None,
    };
    (dir, runtime, input)
}

#[tokio::test]
async fn text_edit_default_approval_stale_cancel_and_disabled_policy_never_write() {
    for outcome in ["reject", "cancel", "stale", "disabled", "observe"] {
        let (dir, runtime, input) = text_fixture();
        let mut events = runtime.subscribe();
        let run_id = Uuid::new_v4();
        if matches!(outcome, "disabled" | "observe") {
            if outcome == "observe" {
                runtime
                    .dispatch(AppCommand::SelectExecutionProfile {
                        profile_id: "observe".into(),
                    })
                    .await
                    .unwrap();
            } else {
                runtime
                    .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                        capability_id: "tool.edit-text".into(),
                        enabled: false,
                    })
                    .await
                    .unwrap();
            }
            assert!(runtime
                .request_tool(ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id,
                    input
                })
                .await
                .is_err());
        } else {
            runtime
                .request_tool(ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id,
                    input,
                })
                .await
                .unwrap();
            let approval_id = drain_events(&mut events)
                .iter()
                .find_map(|e| match e {
                    AppEvent::ApprovalRequested(a) => Some(a.id),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                std::fs::read(dir.path().join("text.txt")).unwrap(),
                b"before\r\nunchanged\r\n"
            );
            match outcome {
                "reject" => runtime
                    .dispatch(AppCommand::RejectTool { approval_id })
                    .await
                    .unwrap(),
                "cancel" => runtime
                    .dispatch(AppCommand::CancelRun { run_id })
                    .await
                    .unwrap(),
                "stale" => {
                    std::fs::write(dir.path().join("text.txt"), "user changed\r\n").unwrap();
                    assert!(matches!(
                        runtime
                            .dispatch(AppCommand::ApproveTool {
                                approval_id,
                                scope: ApprovalScope::Once
                            })
                            .await,
                        Err(RuntimeError::StaleApproval)
                    ));
                }
                _ => unreachable!(),
            }
        }
        let expected = if outcome == "stale" {
            "user changed\r\n"
        } else {
            "before\r\nunchanged\r\n"
        };
        assert_eq!(
            std::fs::read(dir.path().join("text.txt")).unwrap(),
            expected.as_bytes()
        );
        assert!(runtime
            .store
            .change_set(runtime.project_id, run_id)
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
async fn text_edit_journals_snapshots_diff_and_existing_recovery_states() {
    for disk_state in ["before", "after", "conflict"] {
        let (dir, runtime, input) = text_fixture();
        let request = ToolRequest {
            call_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            input,
        };
        let patch = runtime
            .tools
            .prepare_mutations(&request, &runtime.workspace, SandboxScope::WorkspaceWrite)
            .unwrap()
            .unwrap();
        let binding = runtime
            .bindings_for_patch(&patch, FileChangeOrigin::ToolCall, None)
            .unwrap()
            .remove(0);
        let operation = PreparedChangeOperation {
            operation_id: Uuid::new_v4(),
            workspace_key: runtime.workspace_key.clone(),
            project_id: runtime.project_id,
            session_id: runtime.session_id(),
            run_id: request.run_id,
            relative_path: "text.txt".into(),
            kind: FileChangeKind::Modified,
            origin: FileChangeOrigin::ToolCall,
            before: binding.before.as_ref().map(|s| s.reference.clone()),
            after: binding.after.as_ref().map(|s| s.reference.clone()),
            before_bytes: binding.before.as_ref().and_then(|s| s.bytes.clone()),
            after_bytes: binding.after.as_ref().and_then(|s| s.bytes.clone()),
            additions: binding.additions,
            removals: binding.removals,
            line_count_availability: binding.line_count_availability,
            restored_from: None,
        };
        runtime.store.prepare_change_operation(&operation).unwrap();
        let disk = match disk_state {
            "before" => "before\r\nunchanged\r\n",
            "after" => "after\r\nunchanged\r\n",
            _ => "user conflict\r\n",
        };
        std::fs::write(dir.path().join("text.txt"), disk).unwrap();
        // A fresh runtime runs the same startup reconciliation against the durable
        // prepared operation; no compensating write is authorized by recovery.
        let reopened = RuntimeService::new(
            runtime.store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(runtime.workspace.clone()),
            runtime.workspace.clone(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("text.txt")).unwrap(),
            disk.as_bytes()
        );
        let state = reopened
            .store
            .change_operation(runtime.project_id, operation.operation_id)
            .unwrap()
            .unwrap()
            .state;
        assert_eq!(
            state,
            match disk_state {
                "before" => ChangeRecoveryState::NotApplied,
                "after" => ChangeRecoveryState::Finalized,
                _ => ChangeRecoveryState::Conflicted,
            }
        );
        if disk_state == "after" {
            let change_set = runtime
                .store
                .change_set(runtime.project_id, request.run_id)
                .unwrap()
                .unwrap();
            assert_eq!(change_set.files.len(), 1);
            assert_eq!(change_set.files[0].kind, FileChangeKind::Modified);
            assert!(operation.before_bytes.is_some() && operation.after_bytes.is_some());
            let change = argentum_changes::prepare_change(
                "text.txt",
                operation.before_bytes.as_deref(),
                operation.after_bytes.as_deref(),
                MAX_SNAPSHOT_BYTES,
            )
            .unwrap();
            let diff = load_unified_diff(&change, 4096).unwrap();
            assert!(!diff.truncated);
            assert!(diff.text.contains("-before\r\n") && diff.text.contains("+after\r\n"));
            assert_eq!(
                argentum_domain::text_contract::MAX_TEXT_SNAPSHOT_BYTES,
                MAX_SNAPSHOT_BYTES
            );
        }
    }
}

#[test]
fn edit_transport_decodes_source_escapes_once_without_guessing_literal_controls() {
    let literal_source = r#"let escaped = ('\t', '\\'); let value = "\n";"#;
    let physical_tab_source = "let escaped = '\t';";
    for (identity, source) in [
        ("literal-source", literal_source),
        ("physical-tab", physical_tab_source),
    ] {
        let arguments = serde_json::json!({
            "path": "source.rs", "expected_sha256": "a".repeat(64),
            "start_line": 1, "line_count": 1, "replacement_lines": [source],
        });
        let call = ModelToolCall {
            id: identity.into(),
            name: "edit_text".into(),
            arguments: arguments.to_string(),
        };
        let ToolInput::EditText {
            replacement_lines: Some(lines),
            ..
        } = parse_model_tool_input(&call).unwrap()
        else {
            panic!("expected the existing line edit form");
        };
        assert_eq!(lines, [source]);
    }
    let call = ModelToolCall {
        id: "literal-exact-source".into(),
        name: "edit_text".into(),
        arguments: serde_json::json!({
            "path": "source.rs", "expected_sha256": "a".repeat(64),
            "old_text": "old", "new_text": literal_source,
        })
        .to_string(),
    };
    let ToolInput::EditText {
        new_text: Some(text),
        ..
    } = parse_model_tool_input(&call).unwrap()
    else {
        panic!("expected the existing exact edit form");
    };
    assert_eq!(text, literal_source);
    assert!(text.contains(r#"'\t'"#));
    assert!(!text.contains('\t'));
}

#[test]
fn text_model_schema_parser_and_action_binding_cover_range_revision_and_edit() {
    let read = ModelToolCall { id: "read".into(), name: "read_text".into(), arguments: r#"{"path":"text.txt","start_byte":9,"end_byte":20,"expected_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#.into() };
    let parsed = parse_model_tool_input(&read).unwrap();
    assert!(matches!(
        parsed,
        ToolInput::ReadText {
            start_byte: Some(9),
            end_byte: Some(20),
            ..
        }
    ));
    let base = ToolInput::EditText {
        path: "text.txt".into(),
        expected_sha256: Sha256Digest("a".repeat(64)),
        old_text: Some("old".into()),
        new_text: Some("new".into()),
        newline_mode: argentum_domain::text_contract::TextNewlineMode::Exact,
        start_line: None,
        line_count: None,
        replacement_lines: None,
    };
    let base_binding = tool_binding(&base).unwrap();
    assert!(serde_json::to_value(&base)
        .unwrap()
        .get("newline_mode")
        .is_none());
    for field in ["path", "expected_sha256", "old_text", "new_text"] {
        let mut value = serde_json::to_value(&base).unwrap();
        value[field] = serde_json::json!("different");
        let changed = serde_json::from_value(value).unwrap();
        assert_ne!(tool_binding(&changed).unwrap(), base_binding);
    }
    let legacy = ModelToolCall {
        id: "legacy".into(),
        name: "edit_text".into(),
        arguments: r#"{"path":"text.txt","expected_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","old_text":"old","new_text":"new"}"#.into(),
    };
    assert!(matches!(
        parse_model_tool_input(&legacy).unwrap(),
        ToolInput::EditText {
            newline_mode: argentum_domain::text_contract::TextNewlineMode::Exact,
            ..
        }
    ));
    let preserve = ModelToolCall {
        id: "preserve".into(),
        name: "edit_text".into(),
        arguments: r#"{"path":"text.txt","expected_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","old_text":"old\nline","new_text":"new\nline","newline_mode":"preserve"}"#.into(),
    };
    let preserve = parse_model_tool_input(&preserve).unwrap();
    assert!(matches!(
        preserve,
        ToolInput::EditText {
            newline_mode: argentum_domain::text_contract::TextNewlineMode::Preserve,
            ..
        }
    ));
    assert_ne!(tool_binding(&preserve).unwrap(), base_binding);
    assert_eq!(
        serde_json::to_value(&preserve).unwrap()["newline_mode"],
        "preserve"
    );
    assert_eq!(tool_action(&base), "Edit exact text");
    assert_eq!(tool_action(&preserve), "Edit text preserving line endings");
    let descriptor = ToolRegistry::with_builtins(WorkspaceManager::new(
        CapabilityBroker::new(
            tempfile::tempdir().unwrap().path(),
            ApprovalPolicy::default(),
        )
        .unwrap(),
    ))
    .descriptors()
    .into_iter()
    .find(|descriptor| descriptor.id == "edit_text")
    .unwrap();
    let definition = model_tool_definition(&descriptor).unwrap();
    assert_eq!(
        definition.parameters["properties"]["newline_mode"]["enum"],
        serde_json::json!(["exact", "preserve"])
    );
    assert!(!definition.parameters["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "newline_mode"));
}

#[test]
fn structural_edit_schema_parser_and_preparation_reject_mixing_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("source.rs"),
        b"before\r\nold\r\nafter\r\n",
    )
    .unwrap();
    let workspace = WorkspaceManager::new(
        CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap(),
    );
    let read = workspace
        .read_text_lines_with_scope("source.rs", 2, 1, None, SandboxScope::WorkspaceWrite)
        .unwrap();
    let selection = read.edit_selection.expect("complete edit selection");
    let registry = ToolRegistry::with_builtins(workspace.clone());
    let descriptor = registry
        .descriptors()
        .into_iter()
        .find(|item| item.id == "edit_text")
        .unwrap();
    let schema = model_tool_definition(&descriptor).unwrap().parameters;
    assert_eq!(schema["oneOf"].as_array().unwrap().len(), 2);
    assert_eq!(schema["properties"]["replacement_lines"]["maxItems"], 200);

    let valid = serde_json::json!({
        "path": "source.rs", "expected_sha256": selection.expected_sha256,
        "start_line": selection.start_line, "line_count": selection.line_count,
        "replacement_lines": [r#"let value = "\r\n";"#, "\tprintln!(\"中\");"],
    });
    let call = |id: &str, value: serde_json::Value| ModelToolCall {
        id: id.into(),
        name: "edit_text".into(),
        arguments: value.to_string(),
    };
    for (name, mut invalid) in [
        ("mixed", valid.clone()),
        ("physical-newline", valid.clone()),
        ("empty-deletion", valid.clone()),
    ] {
        match name {
            "mixed" => invalid["old_text"] = serde_json::json!("old"),
            "physical-newline" => invalid["replacement_lines"][0] = serde_json::json!("x\ny"),
            _ => invalid["replacement_lines"] = serde_json::json!([]),
        }
        let error = parse_model_tool_input(&call(name, invalid)).unwrap_err();
        assert!(matches!(
            error.code,
            PreflightCode::InvalidArguments | PreflightCode::InvalidSelector
        ));
        assert!(error.issue.is_some());
        assert_eq!(
            std::fs::read(directory.path().join("source.rs")).unwrap(),
            b"before\r\nold\r\nafter\r\n"
        );
    }
    let input = parse_model_tool_input(&call("corrected", valid)).unwrap();
    assert_eq!(tool_action(&input), "Replace selected text lines");
    let serialized = serde_json::to_value(&input).unwrap();
    assert!(serialized.get("old_text").is_none() && serialized.get("new_text").is_none());
    let request = ToolRequest {
        call_id: Uuid::new_v4(),
        run_id: Uuid::new_v4(),
        input,
    };
    let patch = registry
        .prepare_mutations(&request, &workspace, SandboxScope::WorkspaceWrite)
        .unwrap()
        .unwrap();
    assert_eq!(
        patch.mutations[0].after.as_deref(),
        Some(
            &b"before\r\nlet value = \"\\r\\n\";\r\n\tprintln!(\"\xe4\xb8\xad\");\r\nafter\r\n"[..]
        )
    );
    assert_eq!(
        std::fs::read(directory.path().join("source.rs")).unwrap(),
        b"before\r\nold\r\nafter\r\n"
    );
}

fn write_nested(root: &std::path::Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn workspace_at(root: &std::path::Path) -> WorkspaceManager {
    WorkspaceManager::new(CapabilityBroker::new(root, ApprovalPolicy::default()).unwrap())
}

fn assert_search_visibility(workspace: &WorkspaceManager, expected: &[&str]) {
    let names = workspace
        .search_filenames_bounded_report(".", "*.txt", 50)
        .unwrap();
    let text = workspace
        .search_text_bounded_report(".", "needle", 50)
        .unwrap();
    assert!(names.complete && text.complete);
    assert!(names.incomplete_reasons.is_empty() && text.incomplete_reasons.is_empty());
    let mut expected = expected
        .iter()
        .map(|path| path.to_string())
        .collect::<Vec<_>>();
    expected.sort();
    for paths in [
        names
            .matches
            .into_iter()
            .map(|item| item.path)
            .collect::<Vec<_>>(),
        text.matches
            .into_iter()
            .map(|item| item.path)
            .collect::<Vec<_>>(),
    ] {
        let mut paths = paths;
        paths.sort();
        assert_eq!(paths, expected);
    }
}

#[test]
fn slash_anchored_file_pattern_applies_only_at_the_ignore_file_directory() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_nested(root, ".gitignore", "/only.txt\n");
    write_nested(root, "only.txt", "needle top\n");
    write_nested(root, "nested/only.txt", "needle deep\n");
    let workspace = workspace_at(root);
    let report = workspace
        .search_filenames_bounded_report(".", "only.txt", 50)
        .unwrap();
    let paths = report
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert!(
        paths.iter().any(|path| path.ends_with("nested/only.txt")),
        "an anchored /only.txt must not hide nested/only.txt: {paths:?}"
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("only.txt") && !path.ends_with("nested/only.txt")),
        "an anchored /only.txt must hide the sibling only.txt: {paths:?}"
    );
    let text = workspace
        .search_text_bounded_report_with_options_and_scope_and_cancellation(
            ".",
            "needle",
            50,
            WorkspaceTextSearchOptions {
                mode: argentum_domain::TextSearchMode::Literal,
                before_context: 0,
                after_context: 0,
            },
            SandboxScope::WorkspaceWrite,
            || false,
        )
        .unwrap_or_else(|error| panic!("text search failed: {error:?}"));
    assert_eq!(
        text.matches
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>(),
        ["nested/only.txt"]
    );
    assert_search_visibility(&workspace, &["nested/only.txt"]);
}

#[test]
fn middle_slash_file_pattern_stays_relative_to_the_ignore_directory() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_nested(root, ".gitignore", "src/only.txt\n");
    write_nested(root, "src/only.txt", "needle direct\n");
    write_nested(root, "deeper/src/only.txt", "needle deep\n");
    write_nested(root, "src/other.txt", "needle other\n");
    let workspace = workspace_at(root);
    let report = workspace
        .search_filenames_bounded_report(".", "only.txt", 50)
        .unwrap();
    let paths = report
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert!(
        !paths.iter().any(|path| path == "src/only.txt"),
        "src/only.txt must hide the direct src/only.txt: {paths:?}"
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("deeper/src/only.txt")),
        "src/only.txt must not hide deeper/src/only.txt: {paths:?}"
    );
    assert_search_visibility(&workspace, &["deeper/src/only.txt", "src/other.txt"]);
}

#[test]
fn leading_slash_middle_slash_pattern_does_not_hide_deeper_paths() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_nested(root, ".gitignore", "/src/only.txt\n");
    write_nested(root, "src/only.txt", "needle direct\n");
    write_nested(root, "deeper/src/only.txt", "needle deep\n");
    let workspace = workspace_at(root);
    let report = workspace
        .search_filenames_bounded_report(".", "only.txt", 50)
        .unwrap();
    let paths = report
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert!(
        !paths.iter().any(|path| path == "src/only.txt"),
        "/src/only.txt must hide src/only.txt: {paths:?}"
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("deeper/src/only.txt")),
        "/src/only.txt must not hide deeper/src/only.txt: {paths:?}"
    );
    assert_search_visibility(&workspace, &["deeper/src/only.txt"]);
}

#[test]
fn recursive_directory_pattern_remains_recursive_and_unanchored() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_nested(root, ".gitignore", "cache/\n");
    write_nested(root, "cache/one.txt", "needle one\n");
    write_nested(root, "nested/cache/two.txt", "needle two\n");
    write_nested(root, "keep.txt", "needle keep\n");
    let workspace = workspace_at(root);
    let report = workspace
        .search_filenames_bounded_report(".", "*.txt", 50)
        .unwrap();
    let paths = report
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert!(
        !paths.iter().any(|path| path.starts_with("cache/")),
        "cache/ must prune the top-level cache directory: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path.contains("nested/cache/")),
        "unanchored cache/ must stay recursive: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path == "keep.txt"),
        "unrelated files must remain visible: {paths:?}"
    );
    assert_search_visibility(&workspace, &["keep.txt"]);
}

#[test]
fn leading_slash_directory_pattern_is_directory_only_at_one_level() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_nested(root, ".gitignore", "/cache/\n");
    write_nested(root, "cache/one.txt", "needle one\n");
    write_nested(root, "nested/cache/two.txt", "needle two\n");
    let workspace = workspace_at(root);
    let report = workspace
        .search_filenames_bounded_report(".", "*.txt", 50)
        .unwrap();
    let paths = report
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert!(
        !paths.iter().any(|path| path.starts_with("cache/")),
        "/cache/ must prune the top-level cache directory: {paths:?}"
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("nested/cache/two.txt")),
        "/cache/ must not prune a nested cache directory: {paths:?}"
    );
    assert_search_visibility(&workspace, &["nested/cache/two.txt"]);
}

#[test]
fn nested_ignore_files_anchor_to_their_own_directory_not_siblings() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_nested(root, "alpha/.gitignore", "/shared.txt\n");
    write_nested(root, "alpha/shared.txt", "needle alpha\n");
    write_nested(root, "beta/shared.txt", "needle beta\n");
    write_nested(root, "alpha/deep/shared.txt", "needle alpha deep\n");
    let workspace = workspace_at(root);
    let report = workspace
        .search_filenames_bounded_report(".", "shared.txt", 50)
        .unwrap();
    let paths = report
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert!(
        !paths.iter().any(|path| path == "alpha/shared.txt"),
        "the nested /shared.txt rule hides alpha/shared.txt: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path == "beta/shared.txt"),
        "the nested rule must not affect the sibling beta directory: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path == "alpha/deep/shared.txt"),
        "the nested anchored rule must not hide alpha/deep/shared.txt: {paths:?}"
    );
    assert_search_visibility(&workspace, &["alpha/deep/shared.txt", "beta/shared.txt"]);
}

#[test]
fn ordered_negation_restores_visibility_of_deeper_anchored_paths() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_nested(root, ".gitignore", "*.txt\n!nested/only.txt\n");
    write_nested(root, "only.txt", "needle top\n");
    write_nested(root, "nested/only.txt", "needle deep\n");
    write_nested(root, "deeper/nested/only.txt", "needle deeper\n");
    let workspace = workspace_at(root);
    let report = workspace
        .search_filenames_bounded_report(".", "only.txt", 50)
        .unwrap();
    let paths = report
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert!(
        paths.iter().any(|path| path.ends_with("nested/only.txt")),
        "the ordered negation must restore nested/only.txt: {paths:?}"
    );
    assert!(report.incomplete_reasons.is_empty());
    assert_search_visibility(&workspace, &["nested/only.txt"]);
}

#[test]
fn nested_anchored_negation_does_not_restore_sibling_or_deeper_files() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_nested(root, ".gitignore", "*.txt\n");
    write_nested(root, "nested/.gitignore", "!/keep.txt\n");
    write_nested(root, "keep.txt", "needle top\n");
    write_nested(root, "nested/keep.txt", "needle keep\n");
    write_nested(root, "nested/deeper/keep.txt", "needle deeper\n");
    write_nested(root, "sibling/keep.txt", "needle sibling\n");
    let workspace = workspace_at(root);
    let report = workspace
        .search_filenames_bounded_report(".", "keep.txt", 50)
        .unwrap();
    let paths = report
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert!(
        paths.iter().any(|path| path == "nested/keep.txt"),
        "the local !/keep.txt restores its direct file: {paths:?}"
    );
    assert!(
        !paths
            .iter()
            .any(|path| path == "nested/deeper/keep.txt" || path == "sibling/keep.txt"),
        "local anchored negation must not restore deeper or sibling files: {paths:?}"
    );
    assert_search_visibility(&workspace, &["nested/keep.txt"]);
}

#[test]
fn hard_exclusions_ignore_negation_and_fixed_git_target_pruning() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write_nested(root, ".gitignore", "!/.git/\n!/target/\n!/nested/target/\n");
    write_nested(root, "target/hidden.txt", "needle hidden\n");
    write_nested(root, ".git/secret.txt", "needle secret\n");
    write_nested(root, "nested/target/hidden.txt", "needle nested\n");
    write_nested(root, "visible.txt", "needle visible\n");
    let workspace = workspace_at(root);
    let report = workspace
        .search_filenames_bounded_report(".", "*.txt", 50)
        .unwrap();
    let paths = report
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert!(
        !paths.iter().any(|path| path.contains("target/")),
        "the fixed target exclusion must stay pruned: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path.contains(".git/")),
        "the fixed .git exclusion must stay pruned: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path == "visible.txt"),
        "unrelated source must remain visible: {paths:?}"
    );
    let focused = workspace
        .search_text_bounded_report_with_options_and_scope_and_cancellation(
            "target",
            "needle",
            50,
            WorkspaceTextSearchOptions {
                mode: argentum_domain::TextSearchMode::Literal,
                before_context: 0,
                after_context: 0,
            },
            SandboxScope::WorkspaceWrite,
            || false,
        )
        .unwrap_or_else(|error| panic!("focused text search failed: {error:?}"));
    assert_eq!(focused.matches.len(), 0);
    assert!(focused.policy_directories_pruned >= 1);
    assert_search_visibility(&workspace, &["visible.txt"]);
}

fn write_sparse(path: &std::path::Path, len: u64) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let file = std::fs::File::create(path).unwrap();
    file.set_len(len).unwrap();
}

#[test]
fn filename_discovery_is_independent_of_content_length_and_text_stays_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    // Exactly the text-content cap remains eligible for filename discovery.
    let boundary = argentum_workspaces::MAX_SEARCH_FILE_BYTES;
    write_nested(root, "small.txt", "needle small\n");
    write_sparse(&root.join("at_cap.txt"), boundary);
    write_sparse(&root.join("above_cap.txt"), boundary + 1);
    write_sparse(&root.join("far_above.txt"), 16 * 1024 * 1024 + 1);
    write_sparse(&root.join("nested/above_cap_nested.txt"), boundary + 1);
    let workspace = workspace_at(root);

    let recursive = workspace
        .search_filenames_bounded_report(".", "*.txt", 50)
        .unwrap();
    assert!(
        recursive.complete && recursive.incomplete_reasons.is_empty(),
        "filename discovery must not be incomplete for oversized content: {:?}",
        recursive.incomplete_reasons
    );
    assert_eq!(recursive.files_read, 0);
    assert_eq!(recursive.bytes_read, 0);
    assert_eq!(recursive.oversized_files_skipped, 0);
    let mut names = recursive
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        [
            "above_cap.txt",
            "at_cap.txt",
            "far_above.txt",
            "nested/above_cap_nested.txt",
            "small.txt",
        ]
    );

    // Direct filename search on one above-cap file stays discoverable and complete.
    let direct = workspace
        .search_filenames_bounded_report("far_above.txt", "far_above.txt", 50)
        .unwrap();
    assert!(direct.complete && direct.incomplete_reasons.is_empty());
    assert_eq!(direct.files_read, 0);
    assert_eq!(direct.bytes_read, 0);
    assert_eq!(direct.oversized_files_skipped, 0);
    assert_eq!(direct.matches.len(), 1);
    assert_eq!(direct.matches[0].path, "far_above.txt");

    // Text search keeps its existing oversized-content rejection unchanged.
    let text = workspace
        .search_text_bounded_report(".", "needle", 50)
        .unwrap();
    assert!(!text.complete);
    assert_eq!(
        text.stop_reason,
        argentum_workspaces::WorkspaceSearchStopReason::OversizedContent
    );
    assert!(text.oversized_files_skipped >= 1);
    assert!(
        text.incomplete_reasons
            .iter()
            .any(|reason| reason == "oversized_content"),
        "text search must still report oversized content: {:?}",
        text.incomplete_reasons
    );
    let text_paths = text
        .matches
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    assert_eq!(text_paths, ["small.txt"]);
}
