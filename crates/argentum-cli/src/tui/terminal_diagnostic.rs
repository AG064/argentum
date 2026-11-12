//! Explicit provider-free fixture. There is no environment switch or normal
//! chat hook that can enable raw text capture.
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::*;

const MAX_RECORDS: usize = 2048;
const MAX_BYTES: usize = 512 * 1024;
const LIMIT: Duration = Duration::from_secs(90);

#[derive(Clone)]
pub(super) struct Trace(Arc<Mutex<Log>>);

struct Log {
    start: Instant,
    bytes: usize,
    mapped: u64,
    records: Vec<Value>,
    stopped: bool,
}

impl Trace {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Log {
            start: Instant::now(),
            bytes: 0,
            mapped: 0,
            records: Vec::new(),
            stopped: false,
        })))
    }

    fn record(&self, mut value: Value) {
        let Ok(mut log) = self.0.lock() else {
            return;
        };
        if log.stopped {
            return;
        }
        value["elapsed_ms"] = json!(log.start.elapsed().as_millis());
        let bytes = value.to_string().len() + 1;
        if log.records.len() >= MAX_RECORDS
            || log.bytes + bytes > MAX_BYTES - 4096
            || log.start.elapsed() >= LIMIT
        {
            log.stopped = true;
            return;
        }
        log.bytes += bytes;
        log.records.push(value);
    }

    pub fn native(&self, sequence: u64, event: &Event, input: Option<&InputEvent>) {
        let mapped = if let Ok(mut log) = self.0.lock() {
            if input.is_some() {
                log.mapped += 1;
            }
            input.map(|_| log.mapped)
        } else {
            None
        };
        let native = match event {
            Event::Key(key) => {
                json!({"key":format!("{:?}", key.code), "kind":format!("{:?}", key.kind), "modifiers":key.modifiers.bits()})
            }
            Event::Paste(text) => json!({"paste":sample(text), "bytes":text.len()}),
            other => json!({"event":format!("{other:?}")}),
        };
        self.record(
            json!({"stage":"native_to_mapped", "sequence":sequence, "mapped_id":mapped,
            "reader_thread":format!("{:?}", thread::current().id()), "native":native,
            "action": input.map(action_sample)}),
        );
    }

    fn expired(&self) -> bool {
        self.0
            .lock()
            .map_or(true, |log| log.stopped || log.start.elapsed() >= LIMIT)
    }

    fn finish(&self, mut file: File, summary: Value) -> io::Result<()> {
        let mut log = self
            .0
            .lock()
            .map_err(|_| io::Error::other("diagnostic lock failed"))?;
        log.stopped = true;
        for record in &log.records {
            writeln!(file, "{record}")?;
        }
        writeln!(file, "{summary}")?;
        file.flush()
    }
}

fn sample(text: &str) -> String {
    text.chars().take(256).collect()
}

fn action_sample(input: &InputEvent) -> Value {
    match input {
        InputEvent::Text(text) => json!({"text":sample(text), "bytes":text.len()}),
        other => json!(format!("{other:?}")),
    }
}

fn console_snapshot() -> Value {
    #[cfg(windows)]
    {
        windows_console::snapshot()
    }
    #[cfg(not(windows))]
    {
        json!({"reader_api":"crossterm VT", "size":terminal::size().ok(), "cursor_query":"omitted to preserve single input reader"})
    }
}

fn binary_hash(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Run the real terminal input mapper, editor, renderer and guard without a
/// CommandHost, provider, credential store, configuration discovery or database.
pub async fn run_terminal_diagnostic<F, Fut>(output: &Path, setup_probe: F) -> io::Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = io::Result<String>>,
{
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other("terminal-diagnostic requires terminal stdin and stdout; redirected input is not a native diagnostic"));
    }
    if !output.is_absolute() || output.parent().is_none_or(|p| !p.is_dir()) {
        return Err(io::Error::other(
            "diagnostic output requires an absolute file path in an existing directory",
        ));
    }
    // create_new refuses overwrite, including a pre-existing final symlink.
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)?;
    let executable = std::env::current_exe()?;
    let hash = binary_hash(&executable)?;
    let before_setup = console_snapshot();
    println!("Synthetic diagnostic armed. Use test text only. No providers, credentials or saved setup.\nType abc and Enter at the setup-reader prompt. Then raw capture runs for 90 seconds; F10 stops.");
    let setup_text = setup_probe().await?;
    let trace = Trace::new();
    trace.record(json!({"stage":"startup", "exe":executable, "sha256":hash,
        "crossterm":"0.28.1", "features":["bracketed-paste","windows","events"],
        "windows_terminal_marker":std::env::var_os("WT_SESSION").is_some(),
        "transport_note":"Windows uses ReadConsoleInputW even when hosted in ConPTY; host identity beyond WT_SESSION presence is unknown",
        "native_repeat_count":"not exposed by locked crossterm", "modes":console_snapshot()}));
    trace.record(json!({"stage":"synthetic_setup_to_chat", "before_modes":before_setup,
        "returned_text":sample(&setup_text), "returned_bytes":setup_text.len(), "after_modes":console_snapshot(),
        "note":"same bounded setup line reader and completed handoff; provider/permission setup not executed"}));
    let mut state = TuiState::new(
        PathBuf::from("synthetic/workspace/long/path/for/viewport/testing/only"),
        true,
    );
    state.project_name = "SYNTHETIC INPUT CAPTURE ON: 90 seconds, F10 stops; no providers".into();
    state.diagnostic_active = true;
    state.project_id = Some(argentum_domain::ProjectId::new_v4());
    state.active_session_id = Some(argentum_domain::SessionId::new_v4());
    state.status =
        "Type abc, Enter, bookkeeper. Test arrows, accents, paste, resize, /setup. F10 stops."
            .into();
    let mut guard = match TerminalGuard::enter() {
        Ok(guard) => guard,
        Err(error) => {
            trace.finish(file, json!({"stage":"terminal_entry_failed", "error":error.to_string(), "modes":console_snapshot()}))?;
            println!("Synthetic capture stopped. Trace: {}", output.display());
            return Err(error);
        }
    };
    trace.record(json!({"stage":"chat_entered", "modes":console_snapshot()}));
    let mut renderer = TtyRenderer::stdout();
    let initial = renderer.render(&state);
    if let Err(error) = initial {
        let restore = guard.restore();
        trace.finish(file, json!({"stage":"initial_frame_failed", "error":error.to_string(), "restore_error":restore.err().map(|e| e.to_string()), "modes":console_snapshot()}))?;
        println!("Synthetic capture stopped. Trace: {}", output.display());
        return Err(error);
    }
    let (mut receiver, stop, reader) = spawn_input_reader(Some(trace.clone()));
    let mut mapped = 0u64;
    let mut accepted = 0u64;
    let stream_run = RunId::new_v4();
    let mut stream_ticks = 0u16;
    let mut dirty = false;
    let mut ticker = tokio::time::interval(Duration::from_millis(33));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result: io::Result<()> = async {
        loop {
            tokio::select! {
                input = receiver.recv() => {
                    let Some(input) = input else { break; };
                    mapped += 1;
                    let before = [state.composer.len(), state.composer_cursor];
                    let action = state.apply_input(input);
                    trace.record(json!({"stage":"editor", "mapped_id":mapped, "before":before,
                        "after":[state.composer.len(),state.composer_cursor], "text":sample(&state.composer)}));
                    match action {
                        TuiAction::Submit(submission) => {
                            accepted += 1;
                            trace.record(json!({"stage":"fixture_submission", "mapped_id":mapped, "id":submission.id,
                                "run_id":submission.run_id, "prompt":sample(&submission.prompt), "bytes":submission.prompt.len(),
                                "provider_dispatch":false}));
                            state.submission_finished(TaskSubmissionResult {id:submission.id, run_id:submission.run_id, admitted:true,
                                terminal_lifecycle:Some(argentum_domain::TaskLifecycle::Complete), result:Ok(())});
                            state.status = format!("Synthetic accepted count: {accepted}. F10 stops; no provider called.");
                            stream_ticks = 30;
                        }
                        TuiAction::Exit => break,
                        TuiAction::Dispatch(_) | TuiAction::CompactContext {..} | TuiAction::InspectContext(_) => {
                            state.status = "Diagnostic does not dispatch commands. F10 stops.".into();
                        }
                        _ => {}
                    }
                    dirty = true;
                }
                _ = ticker.tick() => {
                    if trace.expired() { break; }
                    if stream_ticks > 0 {
                        // A labelled synthetic stream tests frame/input interleaving only.
                        if stream_ticks.is_multiple_of(3) { state.append_assistant_delta(stream_run, " synthetic"); dirty = true; }
                        stream_ticks -= 1;
                    }
                    if dirty {
                        renderer.render(&state)?;
                        trace.record(json!({"stage":"frame", "stats":renderer.stats,
                            "cursor":renderer.previous.as_ref().map(|f| f.cursor),
                            "intended_rows":renderer.previous.as_ref().map(|f| f.rows.iter().take(40).map(|row| sample(row)).collect::<Vec<_>>()),
                            "modes":console_snapshot()}));
                        dirty = false;
                    }
                }
            }
        }
        Ok(())
    }.await;
    stop.store(true, Ordering::Release);
    let _ = reader.join();
    trace.record(json!({"stage":"reader_joined", "modes":console_snapshot()}));
    let restore = guard.restore();
    trace.finish(file, json!({"stage":"stopped", "accepted":accepted, "mapped":mapped, "stats":renderer.stats,
        "modes":console_snapshot(), "result":result.as_ref().err().map(ToString::to_string),
        "restore_error":restore.as_ref().err().map(ToString::to_string),
        "limits":{"seconds":90,"records":MAX_RECORDS,"bytes":MAX_BYTES},
        "coverage":"native fixture with real setup line-reader handoff; no real provider/permission onboarding, provider calls, or external visual verification"}))?;
    println!("Synthetic capture stopped. Trace: {}", output.display());
    result.and(restore)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_is_bounded_and_create_new_cannot_overwrite() {
        let trace = Trace::new();
        for _ in 0..MAX_RECORDS + 3 {
            trace.record(json!({"synthetic":"abc"}));
        }
        assert!(trace.expired());
        assert_eq!(trace.0.lock().unwrap().records.len(), MAX_RECORDS);
        assert!(trace.0.lock().unwrap().bytes <= MAX_BYTES);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trace.jsonl");
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        trace.finish(file, json!({"stage":"stopped"})).unwrap();
        assert!(OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .is_err());
    }
}
