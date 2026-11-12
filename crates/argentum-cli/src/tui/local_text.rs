use argentum_providers::{ModelMessage, ModelMessageRole, ProviderEvent};
use tokio_util::sync::CancellationToken;

use super::*;

/// Explicitly local, text-only and memory-only. No AppCommand is dispatched.
pub async fn run_local_text_session(host: &mut CommandHost) -> Result<(), TuiError> {
    let profile = host
        .provider_profiles()?
        .into_iter()
        .find(|p| p.id == "llama-cpp")
        .ok_or_else(|| io::Error::other("llama.cpp profile is unavailable"))?;
    let mut state = TuiState::new(host.workspace_root().to_path_buf(), true);
    let scope = host.workspace_snapshot()?;
    state.project_id = Some(scope.project.id);
    state.active_session_id = scope.active_session_id;
    state.editor_target = state.submission_target();
    state.local_text_mode = true;
    state.project_name = "Local text chat: no coding tools".into();
    state.provider_label = "llama.cpp (text-only)".into();
    state.model = profile.model;
    state.active_session_title = "memory only".into();
    state.permission_mode = "No tools or file access in this mode".into();
    state.status = "Ready. Enter sends; Ctrl+C cancels; /quit exits.".into();
    let mut guard = TerminalGuard::enter()?;
    let mut renderer = TtyRenderer::stdout();
    renderer.render(&state)?;
    let (mut input, stop, reader) = spawn_input_reader(None);
    let (sender, mut events) = mpsc::channel(32);
    let mut tasks = JoinSet::new();
    let mut settings_tasks = JoinSet::new();
    let mut cancel = CancellationToken::new();
    let mut history: Vec<ModelMessage> = Vec::new();
    let mut answer = String::new();
    let mut dirty = false;
    let mut next_frame = tokio::time::Instant::now();
    let result: Result<(), TuiError> = async {
        loop {
            tokio::select! {
                event = input.recv() => {
                    let Some(event) = event else { break; };
                    if event == InputEvent::CtrlC { cancel.cancel(); }
                    if event == InputEvent::Enter {state.local_text_limit=host.local_handle().local_text_input_capacity()?;}
                    match state.apply_input(event) {
                        TuiAction::Ui(request)=>{
                            let local=host.local_handle();
                            let outcome=match request {
                                menu::Request::Settings(action)=>{super::settings::dispatch(&mut state,&local,&mut settings_tasks,action);Ok(())},
                                menu::Request::Review=>super::review::load_sync(&mut state,&local),
                                menu::Request::Open(view)=>menu::open_view(&mut state,&local,view),
                                menu::Request::ReviewAccess(id)=>menu::review_access(&mut state,&local,&id),
                                menu::Request::CommitAccess=>{
                                    if let Some(review)=state.access_review.take(){
                                        if review.review.busy || state.active_run_id.is_some(){state.info_overlay(menu::View::AccessReview,"Cancel and drain active work, then reopen access.".into());Ok(())}
                                        else {match local.apply_access_review(review).await{Ok(())=>menu::open_view(&mut state,&local,menu::View::Status),Err(error)=>Err(error)}}
                                    } else {Ok(())}
                                }
                            };
                            if let Err(error)=outcome{state.info_overlay(menu::View::Status,error.to_string());}
                        }
                        TuiAction::Dispatch(AppCommand::CancelRun{..})=>cancel.cancel(),
                        TuiAction::Exit => break,
                        TuiAction::LocalText(prompt) => {
                            while !history.is_empty() && (history.len() >= 16 || history.iter().map(|m| m.content.len()).sum::<usize>() + prompt.len() > state.local_text_limit) {
                                history.drain(..history.len().min(2));
                            }
                            let local = host.local_handle();
                            let messages = history.clone();
                            let output = sender.clone();
                            cancel = CancellationToken::new();
                            let signal = cancel.clone();
                            state.active_run_id = Some(RunId::new_v4());
                            state.status = "Generating local text; Ctrl+C cancels".into();
                            answer.clear();
                            tasks.spawn(async move {
                                let result = local.stream_local_text(prompt.clone(), messages, output, signal).await;
                                (prompt, result)
                            });
                        }
                        _ => {}
                    }
                    dirty = true;
                }
                completed = settings_tasks.join_next(), if !settings_tasks.is_empty() => {
                    if let Some(Ok(completed))=completed {if let Some(result)=completed.settings {if let Some(next)=super::settings::finish(&mut state,&host.local_handle(),result).await{super::settings::dispatch(&mut state,&host.local_handle(),&mut settings_tasks,next);}}}
                    dirty=true;
                }
                event = events.recv(), if !tasks.is_empty() => {
                    if let Some(event) = event { apply_text_event(&mut state, &mut answer, &cancel, event); dirty = true; }
                }
                finished = tasks.join_next(), if !tasks.is_empty() => {
                    while let Ok(event) = events.try_recv() { apply_text_event(&mut state, &mut answer, &cancel, event); }
                    match finished {
                        Some(Ok((prompt, outcome))) => {
                            if cancel.is_cancelled() { state.status = "Cancelled. Ready for another message.".into(); }
                            else if let Err(error) = outcome { state.status = format!("Local response failed: {error}"); }
                            else {
                                history.push(ModelMessage { role: ModelMessageRole::User, content: prompt, tool_exchange: None });
                                history.push(ModelMessage { role: ModelMessageRole::Assistant, content: answer.clone(), tool_exchange: None });
                                state.status = "Ready. Local text only; no actions were executed.".into();
                            }
                        }
                        Some(Err(error)) => state.status = format!("Local request ended: {error}"),
                        None => {}
                    }
                    state.active_run_id = None;
                    dirty = true;
                }
                _ = tokio::time::sleep_until(next_frame), if dirty => {
                    super::review::revalidate(&mut state,&host.local_handle());
                    renderer.render(&state)?;
                    next_frame = tokio::time::Instant::now() + Duration::from_millis(33);
                    dirty = false;
                }
            }
        }
        Ok(())
    }.await;
    cancel.cancel();
    state.overlay = None;
    while tasks.join_next().await.is_some() {}
    stop.store(true, Ordering::Release);
    let _ = drain_command_tasks(&mut settings_tasks).await;
    let _ = reader.join();
    let restored = guard.restore().map_err(TuiError::from);
    combine_tty_results(result, restored)
}

fn apply_text_event(
    state: &mut TuiState,
    answer: &mut String,
    cancel: &CancellationToken,
    event: ProviderEvent,
) {
    if cancel.is_cancelled() {
        return;
    }
    match event {
        ProviderEvent::Delta(text) => {
            if answer.len().saturating_add(text.len()) > 512 * 1024 {
                state.append_system(
                    "Response exceeded the bounded in-memory text buffer; generation cancelled.",
                );
                cancel.cancel();
                return;
            }
            answer.push_str(&text);
            super::review::note_output(state);
            if let Some(id) = state.active_run_id {
                state.append_assistant_delta(id, &text);
            }
        }
        ProviderEvent::ToolCall(_) => {
            cancel.cancel();
            state.append_system("Tool output rejected: local chat is text-only.");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn f20d_text_only_review_does_not_enable_tools_or_dispatch_inference() {
        let root = tempfile::tempdir().unwrap();
        let mut host = CommandHost::start_composed(crate::HostConfig::in_memory(root.path()))
            .await
            .unwrap();
        let mut state = TuiState::new(root.path().into(), true);
        let scope = host.workspace_snapshot().unwrap();
        state.project_id = Some(scope.project.id);
        state.active_session_id = scope.active_session_id;
        state.local_text_mode = true;
        let local = host.local_handle();
        let before = local.review_policy_stamp().unwrap();
        let mut events = host.subscribe();
        menu::open_view(&mut state, &local, menu::View::Changes).unwrap();
        assert!(state
            .overlay_rows(100, 24)
            .unwrap()
            .join("\n")
            .contains("No recorded changes"));
        assert!(events.try_recv().is_err());
        assert_eq!(local.review_policy_stamp().unwrap(), before);
        let cancel = CancellationToken::new();
        state.active_run_id = Some(RunId::new_v4());
        apply_text_event(
            &mut state,
            &mut String::new(),
            &cancel,
            ProviderEvent::Delta("synthetic incoming text".into()),
        );
        assert!(state
            .overlay_rows(100, 24)
            .unwrap()
            .join("\n")
            .contains("new output: 1"));
        assert!(state.local_text_mode && state.overlay.is_some());
        host.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn local_api_rejects_tool_history_and_oversized_text_before_backend_access() {
        let directory = tempfile::tempdir().unwrap();
        let mut host = CommandHost::start_composed(crate::HostConfig::in_memory(directory.path()))
            .await
            .unwrap();
        let local = host.local_handle();
        let tool_history = vec![ModelMessage {
            role: ModelMessageRole::Assistant,
            content: "synthetic".into(),
            tool_exchange: Some(argentum_providers::ModelToolExchange {
                chatgpt_continuation: None,
                assistant_continuation: None,
                assistant_content: String::new(),
                assistant_reasoning: String::new(),
                calls: Vec::new(),
                results: Vec::new(),
            }),
        }];
        for (prompt, history) in [
            ("hello".to_owned(), tool_history),
            (
                "x".repeat(local.local_text_input_capacity().unwrap() + 1),
                Vec::new(),
            ),
        ] {
            let (sender, mut receiver) = mpsc::channel(1);
            let error = local
                .stream_local_text(prompt, history, sender, CancellationToken::new())
                .await
                .unwrap_err();
            assert!(error.to_string().contains("configured bounded input limit"));
            assert!(receiver.try_recv().is_err());
        }
        host.shutdown().await.unwrap();
    }

    #[test]
    fn local_chat_never_creates_a_coding_submission_and_keeps_busy_drafts() {
        let mut state = TuiState::new(PathBuf::from("fixture"), true);
        state.local_text_mode = true;
        state.apply_input(InputEvent::Text("hello".into()));
        assert!(
            matches!(state.apply_input(InputEvent::Enter), TuiAction::LocalText(text) if text == "hello")
        );
        assert_eq!(state.followups.totals(), (0, 0));
        state.active_run_id = Some(RunId::new_v4());
        state.apply_input(InputEvent::Text("next message".into()));
        assert!(matches!(
            state.apply_input(InputEvent::Enter),
            TuiAction::Redraw
        ));
        assert_eq!(state.composer(), "next message");
    }
}
