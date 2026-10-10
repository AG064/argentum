#![cfg(windows)]
//! Opt-in installed-model integration. No downloads, saved hosted credentials,
//! user workspace writes, or interference with another model process.
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::{AppCommand, AppEvent, ToolResultState};
use std::time::Duration;

#[derive(Default)]
struct Observed {
    answer: String,
    tools: Vec<(String, ToolResultState, String)>,
    exact_counts: usize,
    run: Option<argentum_domain::RunId>,
}

async fn task(host: &CommandHost, prompt: &str) -> Result<Observed, Box<dyn std::error::Error>> {
    let mut events = host.subscribe();
    let client = host.client();
    let command = client.dispatch(AppCommand::SubmitTask {
        prompt: prompt.into(),
    });
    tokio::pin!(command);
    let mut observed = Observed::default();
    let deadline = tokio::time::sleep(Duration::from_secs(150));
    tokio::pin!(deadline);
    let mut timed_out = false;
    let apply = |observed: &mut Observed, event| match event {
        AppEvent::AssistantDelta { text, .. } => {
            if observed.answer.len() < 128 * 1024 {
                observed.answer.push_str(&text);
            }
        }
        AppEvent::ToolFinished(trace) => {
            observed.run = Some(trace.run_id);
            eprintln!(
                "tool={} result={:?} summary={}",
                trace.tool_id,
                trace.result,
                trace.summary.chars().take(500).collect::<String>()
            );
            observed
                .tools
                .push((trace.tool_id, trace.result, trace.summary));
        }
        AppEvent::ContextRequestAccounted { accounting, .. } => {
            if accounting.method.starts_with("exact_") {
                observed.exact_counts += 1;
            }
            eprintln!(
                "accounting={} input={} reserve={}",
                accounting.method, accounting.accounted_input, accounting.output_reserve
            );
        }
        AppEvent::Error { message, .. } => eprintln!(
            "bounded failure: {}",
            message.chars().take(240).collect::<String>()
        ),
        AppEvent::RunStatusChanged { run_id, .. } => observed.run = Some(run_id),
        _ => {}
    };
    loop {
        tokio::select! {
            result=&mut command=>{result?; break;}
            event=events.recv()=>{if let Ok(event)=event {
                if let AppEvent::ApprovalRequested(request)=&event {
                    // This fixture authorizes file work only in its disposable
                    // WorkspaceWrite scope, never shell/process/network work.
                    if !request.capabilities.is_empty() && request.capabilities.iter().all(|c|*c==argentum_domain::Capability::WriteFiles)
                        && host.execution_policy()?.sandbox_scope==argentum_domain::SandboxScope::WorkspaceWrite {
                        host.local_handle().dispatch(AppCommand::ApproveTool{approval_id:request.id,scope:argentum_domain::ApprovalScope::Once}).await?;
                    } else {
                        host.local_handle().dispatch(AppCommand::RejectTool{approval_id:request.id}).await?;
                    }
                }
                apply(&mut observed,event);
            }}
            _=&mut deadline, if !timed_out => {
                timed_out = true;
                eprintln!("test deadline: explicitly cancelling and draining the owned run");
                if let Some(run_id)=observed.run {host.local_handle().dispatch(AppCommand::CancelRun{run_id}).await?;}
            }
        }
    }
    while let Ok(event) = events.try_recv() {
        apply(&mut observed, event);
    }
    if timed_out {
        return Err("model task timed out; cancelled and drained".into());
    }
    Ok(observed)
}

#[tokio::test]
#[ignore = "explicit installed model ordinary file workflow in a disposable workspace"]
async fn installed_local_model_reliability_creates_and_reads_plain_requested_files(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace)?;
    let mut config = HostConfig::discover_local_only(&workspace)?;
    config.database = Some(root.path().join("state.db"));
    let mut host = CommandHost::start_composed(config).await?;
    host.configure_local_context("llama-cpp", 32768)?;
    let local = host.local_handle();
    local
        .apply_access_review(local.access_review("guided")?)
        .await?;
    let model = host
        .provider_profiles()?
        .into_iter()
        .find(|p| p.id == "llama-cpp")
        .ok_or("local profile missing")?
        .model;
    let prepared = host.prepare_local_runtime(&model).await?;
    let approval = local.approve_local_runtime(prepared.plan())?;
    let client = host.activate_local_runtime(prepared, approval).await?;
    eprintln!(
        "Owned live acceptance pid={} context={} model={}",
        client.lease().pid(),
        client.context_tokens(),
        model
    );
    let result=async {
        let first=task(&host,"Create hello_world.txt in this workspace with the exact text \"Hello world\". Do not add a comma, exclamation mark or any other text. Then read its contents back to verify them.").await?;
        if std::fs::read_to_string(workspace.join("hello_world.txt"))?.trim()!="Hello world" {return Err("model wrote different text than requested".into());}
        let read=if first.tools.iter().any(|(t,s,p)|t=="read_text" && *s==ToolResultState::Succeeded && p.contains("hello_world.txt")){first}else{task(&host,"Read the contents of hello_world.txt using the file-reading tool, then report its exact contents. File metadata is not the file contents.").await?};
        if !read.tools.iter().any(|(t,s,p)|t=="read_text" && *s==ToolResultState::Succeeded && p.contains("hello_world.txt")){return Err("model did not read back the actual file contents".into());}
        let page=task(&host,"Create a simple attractive landing page in index.html in this workspace, with inline CSS and no external dependencies. Read it back to verify that you wrote the file. Do not create a directory named index.html.").await?;
        let content=std::fs::read_to_string(workspace.join("index.html"))?;
        if !content.to_ascii_lowercase().contains("<html") || content.len()<=128 {return Err("model did not write an HTML document".into());}
        let page=if page.tools.iter().any(|(t,s,p)|t=="read_text" && *s==ToolResultState::Succeeded && p.contains("index.html")){page}else{task(&host,"Read the actual contents of index.html using the file-reading tool and report whether the HTML document was written. Do not modify it.").await?};
        if !page.tools.iter().any(|(t,s,p)|t=="read_text" && *s==ToolResultState::Succeeded && p.contains("index.html")){return Err("model did not read back the HTML file after the explicit follow-up".into());}
        eprintln!("LIVE ordinary requests: real TXT and HTML created and read back; no scripted tool-call responses.");
        Ok::<(),Box<dyn std::error::Error>>(())
    }.await;
    let shutdown = host.shutdown().await;
    eprintln!("Live acceptance cleanup succeeded={}", shutdown.is_ok());
    result?;
    shutdown?;
    Ok(())
}

#[tokio::test]
#[ignore = "explicit installed Qwen sidecar tool integration, no downloads"]
async fn installed_local_model_reads_edits_and_verifies_through_the_command_host(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace)?;
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&workspace)
        .output()?;
    std::fs::create_dir(workspace.join("src"))?;
    std::fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname=\"local-coding-fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )?;
    std::fs::write(
        workspace.join("Cargo.lock"),
        "version = 4\n[[package]]\nname = \"local-coding-fixture\"\nversion = \"0.1.0\"\n",
    )?;
    let canary = format!("LOCAL_TOOL_{}", argentum_domain::RunId::new_v4());
    std::fs::write(workspace.join("src/lib.rs"),format!("// {canary}\npub fn answer() -> u32 {{ 41 }}\n#[test] fn check() {{ assert_eq!(answer(),42); }}\n"))?;
    let mut config = HostConfig::discover_local_only(&workspace)?;
    config.database = Some(root.path().join("state.db"));
    let mut host = CommandHost::start_composed(config).await?;
    host.disconnect_approval_responder();
    let local = host.local_handle();
    host.declare_model_budget("llama-cpp", [8192, 8192, 8191, 0, 128])?;
    let model = host
        .provider_profiles()?
        .into_iter()
        .find(|p| p.id == "llama-cpp")
        .ok_or("missing local profile")?
        .model;
    eprintln!("verifying installed model and runtime artifacts");
    let prepared = host.prepare_local_runtime(&model).await?;
    let approval = local.approve_local_runtime(prepared.plan())?;
    eprintln!("activating the approved owned sidecar");
    let client = host.activate_local_runtime(prepared, approval).await?;
    eprintln!(
        "owned sidecar pid={} model={} (installed artifact only)",
        client.lease().pid(),
        model
    );
    let result=async {
        local.apply_access_review(local.access_review("observe")?).await?;
        let read=task(&host,"Call read_text with exactly {\"path\":\"src/lib.rs\"}, without optional fields. Then answer in plain text with the LOCAL_TOOL_ comment from its result. Do not call another tool. Do not guess the comment.").await?;
        if !read.answer.contains(&canary) || !read.tools.iter().any(|(t,s,_)|t=="read_text" && *s==ToolResultState::Succeeded) || read.exact_counts==0 {
            eprintln!("read answer (synthetic): {}",read.answer.chars().take(1000).collect::<String>());
            return Err("real model did not complete the read-tool proof".into());
        }
        local.apply_access_review(local.access_review("verification")?).await?;
        let repaired=task(&host,"Read src/lib.rs and use edit_text with the returned SHA-256 to change only the answer function from 41 to 42. Then call verify with runners [\"cargo-test-locked\"]. Do not execute arbitrary commands. Finish only after the typed verification result.").await?;
        let file=std::fs::read_to_string(workspace.join("src/lib.rs"))?;
        let final_result=serde_json::to_value(host.final_run_result(repaired.run.ok_or("missing run identity")?)?)?;
        let passed=final_result["verification"]["items"].as_array().is_some_and(|items|items.iter().any(|item|item["runner"]=="cargo-test-locked" && item["state"]=="passed"));
        if !file.contains(&canary) || !passed
            || !repaired.tools.iter().any(|(t,s,_)|t=="edit_text" && *s==ToolResultState::Succeeded)
            || !repaired.tools.iter().any(|(t,s,_)|t=="verify" && *s==ToolResultState::Succeeded) {
            eprintln!("repair answer (synthetic): {}",repaired.answer.chars().take(1500).collect::<String>());
            eprintln!("tool summaries (synthetic): {:?}",repaired.tools);
            return Err("real model did not complete edit and fixed-verifier proof".into());
        }
        eprintln!("Real installed local model: read, SHA-bound edit, fixed verification and exact context accounting passed.");
        Ok::<(),Box<dyn std::error::Error>>(())
    }.await;
    if let Err(error) = &result {
        eprintln!("native coding check failed: {error}");
    }
    let shutdown = host.shutdown().await;
    eprintln!("owned sidecar shutdown completed: {}", shutdown.is_ok());
    result?;
    shutdown?;
    Ok(())
}
