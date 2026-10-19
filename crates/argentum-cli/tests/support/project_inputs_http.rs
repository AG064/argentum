//! Operator-supplied real Node project control. No external provider or credentials.
use super::*;
use argentum_domain::VerificationRunnerId;
use sha2::{Digest, Sha256};
use std::path::Path;

// Metadata readback hashes the real installed project. This operator-only
// control has a bounded watchdog separate from tiny synthetic fixture commands.
fn run(fixture: &CliFixture, args: &[&str]) -> Output {
    let mut child = ChildGuard(
        fixture
            .command(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || read_output(stdout));
    let err = thread::spawn(move || read_output(stderr));
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "real project metadata command timed out: {args:?}"
        );
        thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}

fn next_native(server: &Server, fixture: &CliFixture, result: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if let Ok(request) = server.requests.recv_timeout(Duration::from_millis(100)) {
            let tools = request["tools"].to_string();
            assert!(!tools.contains("\"execute_argv\"") && !tools.contains("\"execute_shell\""));
            return request;
        }
        let output = fixture.root.path().join("evidence").join(result);
        assert!(
            !output.exists(),
            "native process exited before expected request; inspect {}",
            output.display()
        );
        assert!(
            Instant::now() < deadline,
            "real project native request watchdog"
        );
    }
}

fn identity(path: &Path) -> Value {
    // Test fixture construction only. Product snapshots stream bounded handles.
    let bytes = std::fs::read(path).unwrap();
    json!({"sha256":format!("{:x}", Sha256::digest(&bytes)),"bytes":bytes.len()})
}

fn copy_project(source: &Path, destination: &Path, relative: &Path) {
    for entry in std::fs::read_dir(source.join(relative)).unwrap() {
        let entry = entry.unwrap();
        let path = relative.join(entry.file_name());
        if path.components().count() == 1
            && matches!(
                entry.file_name().to_str(),
                Some(".git" | ".agents" | "dist")
            )
        {
            continue;
        }
        // build contains retained .ts sources as well as generated subdirectories.
        if path == Path::new("build/src") || path == Path::new("build/scripts") {
            continue;
        }
        let kind = entry.file_type().unwrap();
        assert!(
            !kind.is_symlink(),
            "qualification copy must not follow links"
        );
        if kind.is_dir() {
            std::fs::create_dir(destination.join(&path)).unwrap();
            copy_project(source, destination, &path);
        } else {
            assert!(kind.is_file());
            std::fs::copy(entry.path(), destination.join(path)).unwrap();
        }
    }
}

fn approve(fixture: &CliFixture) -> argentum_domain::Sha256Digest {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut config = argentum_cli::HostConfig::in_memory(&fixture.workspace);
        config.database = Some(fixture.database.clone());
        let mut host = argentum_cli::CommandHost::start(config).unwrap();
        let runner = VerificationRunnerId::Project("node-project-control".into());
        let review = host.project_verification_review(&runner).unwrap();
        let binding = review.binding.clone();
        // Explicit test-owned authority in this disposable synthetic store only.
        host.local_handle()
            .dispatch(AppCommand::ApproveProjectVerification {
                runner,
                expected_binding: Some(binding.clone()),
            })
            .await
            .unwrap();
        host.shutdown().await.unwrap();
        binding
    })
}

#[cfg(windows)]
#[test]
#[ignore = "requires an operator-provided installed Node site; no network installation"]
fn scoped_node_project_real_cli_verification_and_fresh_process_controls() {
    let source = PathBuf::from(
        std::env::var_os("ARGENTUM_PROJECT_INPUT_SITE").expect("installed fixture path"),
    );
    let node = PathBuf::from(
        std::env::var_os("ARGENTUM_PROJECT_INPUT_NODE").expect("fixed Node executable"),
    );
    let controller = PathBuf::from(
        std::env::var_os("ARGENTUM_PROJECT_INPUT_CONTROLLER").expect("reviewed fixed verifier"),
    );
    let baseline = PathBuf::from(
        std::env::var_os("ARGENTUM_PROJECT_INPUT_BASELINE").expect("public preservation metadata"),
    );
    let server = Server::start();
    let mut fixture = CliFixture::new(&server.endpoint);
    fixture.root.disable_cleanup(true);
    eprintln!(
        "Scoped Node qualification fixture: {}",
        fixture.root.path().display()
    );
    copy_project(&source, &fixture.workspace, Path::new(""));
    std::fs::create_dir(fixture.root.path().join("evidence")).unwrap();
    let agents = fixture.workspace.join(".agents");
    std::fs::create_dir(&agents).unwrap();
    let script = std::fs::read_to_string(controller)
        .unwrap()
        .replace(
            "resolve(import.meta.dirname, '..')",
            "resolve(import.meta.dirname, '../..')",
        )
        .replace(
            "private/baseline.json",
            "workspace/.agents/verification-baseline.json",
        );
    std::fs::write(agents.join("website-check.mjs"), script).unwrap();
    let mut metadata: Value = serde_json::from_slice(&std::fs::read(baseline).unwrap()).unwrap();
    metadata["protected_files"]
        .as_object_mut()
        .unwrap()
        .remove(".agents/verification.yaml");
    std::fs::write(
        agents.join("verification-baseline.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    let fixed = [
        ".agents/website-check.mjs",
        ".agents/verification-baseline.json",
    ]
    .into_iter()
    .map(|path| {
        let mut value = identity(&fixture.workspace.join(path));
        value["path"] = json!(path);
        value
    })
    .collect::<Vec<_>>();
    let config = json!({"version":1,"runners":[{
        "id":"node-project-control","label":"Real Node project suite and build",
        "program":node.file_name().unwrap().to_str().unwrap(),"args":[".agents/website-check.mjs"],"cwd":".","timeout_ms":180000,"output_limit":65536,
        "inputs":{
            "program":identity(&node),"fixed_files":fixed,"dependency_roots":["node_modules"],
            "generated_output_roots":["build/src","build/scripts","dist"],
            "dependency_limits":{"max_files":16384,"max_total_bytes":268435456,"max_file_bytes":134217728},
            "generated_limits":{"max_files":4096,"max_total_bytes":67108864,"max_file_bytes":16777216},
            "inventory_limits":{"max_entries":16384,"max_depth":64}
        }
    }]});
    let configuration = agents.join("verification.yaml");
    std::fs::write(&configuration, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let dll = fixture
        .workspace
        .join("node_modules/@img/sharp-win32-x64/lib/libvips-42.dll");
    assert!(std::fs::metadata(&dll).unwrap().len() > 16 * 1024 * 1024);
    fixture.allow_usability_tools_via_owner();
    let binding = approve(&fixture);
    let goal = json_output(run(&fixture, &["goal","set","--objective","Execute the declared current-source project verification and report its actual result. Do not edit source.","--token-budget","2000000","--tool-budget","16","--time-budget","295","--json"]));
    let id = goal["goal"]["id"].as_str().unwrap();
    json_output(run(
        &fixture,
        &[
            "goal",
            "limits",
            "--goal-id",
            id,
            "--limits",
            "2000000,8,4,16,295,1000000,8,16,295",
            "--json",
        ],
    ));
    json_output(run(
        &fixture,
        &[
            "goal",
            "criteria",
            "--goal-id",
            id,
            "--runners",
            "project:node-project-control",
            "--json",
        ],
    ));
    let ready = r#"{"status":"ready","remaining":[]}"#;
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| {
            let output = deliver(&fixture, id, "4");
            std::fs::write(
                fixture
                    .root
                    .path()
                    .join("evidence/positive-native-stdout.json"),
                &output.stdout,
            )
            .unwrap();
            std::fs::write(
                fixture
                    .root
                    .path()
                    .join("evidence/positive-native-stderr.txt"),
                &output.stderr,
            )
            .unwrap();
            let value: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
            eprintln!(
                "Scoped Node native process returned {}; state {}; reason {}",
                output.status, value["invocation"]["state"], value["invocation"]["stop_reason"]
            );
            output
        });
        eprintln!("Scoped Node waiting for coding request");
        next_native(&server, &fixture, "positive-native-stdout.json");
        text(&server, ready, true);
        eprintln!("Scoped Node waiting for verifier feedback");
        let feedback = next_native(&server, &fixture, "positive-native-stdout.json");
        let body = feedback["messages"].to_string();
        assert!(body.contains("native_verifier_feedback"), "{body}");
        assert!(body.contains("passed"), "{body}");
        std::fs::write(
            fixture.root.path().join("evidence/model-feedback.json"),
            serde_json::to_vec_pretty(&feedback).unwrap(),
        )
        .unwrap();
        text(&server, ready, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], true, "{result}");
    let reopened = json_output(run(&fixture, &["goal", "status", "--json"]));
    assert_eq!(
        reopened["supervision"]["completion_current"], true,
        "{reopened}"
    );
    let check = json_output(run(&fixture, &["verification", "status", "--json"]));
    let positive_check = check.clone();
    assert!(
        check["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["fresh"] == true && v["evidence"]["state"] == "passed"),
        "{check}"
    );
    let receipt: Value = serde_json::from_slice(
        &std::fs::read(fixture.root.path().join("evidence/latest-fixed-check.json")).unwrap(),
    )
    .unwrap();
    assert!(receipt["steps"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["result"] == "passed"));
    let lock = std::fs::read(fixture.workspace.join("package-lock.json")).unwrap();
    let generated = fixture.workspace.join("dist/js/projects-filter.js");
    let original = std::fs::read(&generated).unwrap();
    std::fs::write(&generated, b"substituted generated output").unwrap();
    let stale = json_output(run(&fixture, &["goal", "status", "--json"]));
    assert_ne!(stale["supervision"]["completion_current"], true, "{stale}");
    let check = json_output(run(&fixture, &["verification", "status", "--json"]));
    assert!(check["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["fresh"] == false));
    std::fs::write(&generated, original).unwrap();
    let dependency = fixture.workspace.join("node_modules/sharp/package.json");
    let dependency_before = std::fs::read(&dependency).unwrap();
    let mut changed = dependency_before.clone();
    changed.extend_from_slice(b"\n");
    std::fs::write(&dependency, changed).unwrap();
    assert_eq!(
        lock,
        std::fs::read(fixture.workspace.join("package-lock.json")).unwrap()
    );
    let stale = json_output(run(&fixture, &["goal", "status", "--json"]));
    assert_ne!(stale["supervision"]["completion_current"], true, "{stale}");
    std::fs::write(&dependency, dependency_before).unwrap();
    let retained_build_source = fixture.workspace.join("build/prerender.ts");
    let source_before = std::fs::read(&retained_build_source).unwrap();
    let mut changed_source = source_before.clone();
    changed_source.extend_from_slice(b"\n");
    std::fs::write(&retained_build_source, changed_source).unwrap();
    let stale = json_output(run(&fixture, &["goal", "status", "--json"]));
    assert_ne!(
        stale["supervision"]["completion_current"], true,
        "mixed build directory source must remain observed: {stale}"
    );
    std::fs::write(&retained_build_source, source_before).unwrap();
    let fixed_path = agents.join("website-check.mjs");
    let fixed_before = std::fs::read(&fixed_path).unwrap();
    std::fs::write(&fixed_path, b"throw new Error('unapproved controller')").unwrap();
    let unavailable = json_output(run(&fixture, &["goal", "status", "--json"]));
    assert_ne!(
        unavailable["supervision"]["completion_current"], true,
        "{unavailable}"
    );
    std::fs::write(&fixed_path, fixed_before).unwrap();
    let mut changed_config = config.clone();
    changed_config["runners"][0]["inputs"]["generated_output_roots"] = json!(["dist"]);
    std::fs::write(
        &configuration,
        serde_json::to_vec_pretty(&changed_config).unwrap(),
    )
    .unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut configuration = argentum_cli::HostConfig::in_memory(&fixture.workspace);
        configuration.database = Some(fixture.database.clone());
        let mut host = argentum_cli::CommandHost::start(configuration).unwrap();
        let runner = VerificationRunnerId::Project("node-project-control".into());
        let changed = host.project_verification_review(&runner).unwrap();
        assert_ne!(changed.binding, binding);
        assert!(host
            .local_handle()
            .dispatch(AppCommand::ApproveProjectVerification {
                runner,
                expected_binding: Some(binding.clone())
            })
            .await
            .is_err());
        host.shutdown().await.unwrap();
    });
    let stale = json_output(run(&fixture, &["goal", "status", "--json"]));
    assert_ne!(stale["supervision"]["completion_current"], true, "{stale}");
    std::fs::write(&configuration, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let current = json_output(run(&fixture, &["goal", "status", "--json"]));
    assert_eq!(
        current["supervision"]["completion_current"], true,
        "{current}"
    );
    std::fs::rename(
        fixture.root.path().join("evidence/latest-fixed-check.json"),
        fixture
            .root
            .path()
            .join("evidence/positive-fixed-check.json"),
    )
    .unwrap();
    std::fs::write(fixture.workspace.join("tests/zzzz-scope-required-failure.test.mjs"),"import { test } from 'node:test'; import assert from 'node:assert/strict'; test('required failure',()=>assert.fail('SCOPED_NATIVE_REQUIRED_FAILURE')); test('later passing control',()=>assert.ok(true));\n").unwrap();
    let failed_goal = json_output(run(
        &fixture,
        &[
            "goal",
            "set",
            "--objective",
            "Report the real required project-test failure. Do not edit this negative control.",
            "--token-budget",
            "2000000",
            "--tool-budget",
            "16",
            "--time-budget",
            "295",
            "--json",
        ],
    ));
    let failed_id = failed_goal["goal"]["id"].as_str().unwrap();
    json_output(run(
        &fixture,
        &[
            "goal",
            "limits",
            "--goal-id",
            failed_id,
            "--limits",
            // Updating this retained goal does not reset the positive run's
            // three consumed stages. Leave bounded room for failed feedback.
            "2000000,8,8,16,295,1000000,8,16,295",
            "--json",
        ],
    ));
    json_output(run(
        &fixture,
        &[
            "goal",
            "criteria",
            "--goal-id",
            failed_id,
            "--runners",
            "project:node-project-control",
            "--json",
        ],
    ));
    let (failed_output, failed_requests) = thread::scope(|scope| {
        let child = scope.spawn(|| {
            let output = deliver(&fixture, failed_id, "4");
            std::fs::write(
                fixture
                    .root
                    .path()
                    .join("evidence/negative-native-stdout.json"),
                &output.stdout,
            )
            .unwrap();
            std::fs::write(
                fixture
                    .root
                    .path()
                    .join("evidence/negative-native-stderr.txt"),
                &output.stderr,
            )
            .unwrap();
            output
        });
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut count = 0_u64;
        let mut observed_failure = false;
        while !child.is_finished() {
            if let Ok(request) = server.requests.recv_timeout(Duration::from_millis(100)) {
                count += 1;
                assert!(
                    count <= 4,
                    "negative control cannot silently raise its request allowance"
                );
                let body = request["messages"].to_string();
                if body.contains("native_verifier_feedback") {
                    assert!(
                        body.contains("failed") && body.contains("SCOPED_NATIVE_REQUIRED_FAILURE"),
                        "{body}"
                    );
                    observed_failure = true;
                    std::fs::write(
                        fixture
                            .root
                            .path()
                            .join("evidence/model-failure-feedback.json"),
                        serde_json::to_vec_pretty(&request).unwrap(),
                    )
                    .unwrap();
                }
                // Consume the existing bounded no-progress/report protocol,
                // not a rescue instruction or another actual model request.
                let report = if count == 1 && !observed_failure {
                    ready
                } else {
                    r#"{"status":"blocked","remaining":["The public required test failed; no changes are authorized in this control."]}"#
                };
                text(&server, report, true);
            }
            assert!(
                Instant::now() < deadline,
                "negative native control watchdog"
            );
        }
        assert!(
            observed_failure,
            "current failed verifier feedback must reach the actual next request"
        );
        (child.join().unwrap(), count)
    });
    let failed = json_output(failed_output);
    assert_eq!(failed["task_success"], false, "{failed}");
    assert!(
        !fixture
            .root
            .path()
            .join("evidence/latest-fixed-check.json")
            .exists(),
        "failed required test must not create a new success receipt"
    );
    let failed_receipts = json_output(run(&fixture, &["verification", "status", "--json"]));
    assert!(
        failed_receipts["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["evidence"]["state"] == "failed" && v["evidence"]["exit_code"] == 1),
        "{failed_receipts}"
    );
    assert!(server.requests.try_recv().is_err());
    std::fs::write(fixture.root.path().join("evidence/scope-control-summary.json"),serde_json::to_vec_pretty(&json!({
        "schema_version":1,"external_provider_requests":0,"synthetic_requests":2+failed_requests,"real_node_program":node,"program":identity(&node),
        "native_dll":identity(&dll),"current_verification":positive_check,"completion_fresh_process":reopened,
        "failed_native_outcome":failed,"failed_verification_receipts":failed_receipts,
        "negative_controls":["generated_output_substitution","installed_dependency_changed_without_lockfile_change","retained_mixed_build_source_changed","fixed_controller_changed","changed_scope_and_stale_approval_binding","real_required_node_test_failed_despite_later_pass"],
        "restored_completion_current":current["supervision"]["completion_current"]
    })).unwrap()).unwrap();
    println!(
        "Real Node scoped-input CLI controls retained at {}",
        fixture.root.keep().display()
    );
}
