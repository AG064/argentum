use std::{
    collections::BTreeMap,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver as CommandReceiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use argentum_security::AuthorityGeneration;
use tokio::sync::{mpsc as event_channel, oneshot};
use tokio_util::sync::CancellationToken;

use crate::types::{
    prepare_model, reverify_prepared, ActivationApproval, BackendReport, InferenceBounds,
    InferenceConfig, InferenceError, InferenceInputReport, InferenceRequest, PreparedModel,
    StreamEvent, StreamEventKind, VerifiedModelDescriptor,
};

const WORKER_QUEUE_CAPACITY: usize = 8;
const INFERENCE_WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;
static NEXT_WORKER_INSTANCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineFailure {
    message: String,
}

impl EngineFailure {
    pub fn new(message: impl AsRef<str>) -> Self {
        Self {
            message: bounded_message(message.as_ref()),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for EngineFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EngineFailure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineStreamOutcome {
    Completed,
    Cancelled,
}

pub trait InferenceEngine: Send + 'static {
    fn load(&mut self, model: &PreparedModel) -> Result<BackendReport, EngineFailure>;

    fn stream(
        &mut self,
        request: &InferenceRequest,
        cancellation: &CancellationToken,
        emit: &mut dyn FnMut(&str) -> Result<(), EngineFailure>,
    ) -> Result<EngineStreamOutcome, EngineFailure>;

    fn account(
        &mut self,
        _request: &InferenceRequest,
    ) -> Result<InferenceInputReport, EngineFailure> {
        Err(EngineFailure::new("engine input accounting is unsupported"))
    }

    fn unload(&mut self) -> Result<(), EngineFailure>;
}

pub trait InferenceEngineFactory: Send + 'static {
    fn create(self: Box<Self>) -> Result<Box<dyn InferenceEngine>, EngineFailure>;
}

impl<F> InferenceEngineFactory for F
where
    F: FnOnce() -> Result<Box<dyn InferenceEngine>, EngineFailure> + Send + 'static,
{
    fn create(self: Box<Self>) -> Result<Box<dyn InferenceEngine>, EngineFailure> {
        (*self)()
    }
}

pub struct StreamHandle {
    receiver: event_channel::Receiver<StreamEvent>,
    cancellation: CancellationToken,
    terminal_fallback: Arc<Mutex<Option<StreamEvent>>>,
    terminal_seen: bool,
}

impl StreamHandle {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub async fn recv(&mut self) -> Option<StreamEvent> {
        if self.terminal_seen {
            return None;
        }
        while let Some(event) = self.receiver.recv().await {
            let terminal = !matches!(event.kind(), StreamEventKind::Token(_));
            if self.cancellation.is_cancelled() && matches!(event.kind(), StreamEventKind::Token(_))
            {
                continue;
            }
            if terminal {
                self.terminal_seen = true;
            }
            return Some(event);
        }
        if let Ok(mut fallback) = self.terminal_fallback.lock() {
            if let Some(event) = fallback.take() {
                self.terminal_seen = true;
                return Some(event);
            }
        }
        self.terminal_seen = true;
        None
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

pub struct AndroidInferenceModule {
    worker_instance: u64,
    config: InferenceConfig,
    lifecycle_generation: AuthorityGeneration,
    sender: SyncSender<WorkerCommand>,
    worker: Option<JoinHandle<()>>,
    active_streams: Arc<Mutex<BTreeMap<u64, CancellationToken>>>,
    next_stream_id: u64,
    active_model: Option<VerifiedModelDescriptor>,
    backend_report: Option<BackendReport>,
    activation_generation: u64,
    shutdown: bool,
}

impl AndroidInferenceModule {
    /// Keep configuration without creating an engine or worker thread.
    pub fn stopped(
        config: InferenceConfig,
        lifecycle_generation: AuthorityGeneration,
    ) -> Result<Self, InferenceError> {
        if lifecycle_generation.ordinal() == 0 {
            return Err(InferenceError::InvalidGeneration);
        }
        let (sender, _) = mpsc::sync_channel(WORKER_QUEUE_CAPACITY);
        Ok(Self {
            worker_instance: 0,
            config,
            lifecycle_generation,
            sender,
            worker: None,
            active_streams: Arc::new(Mutex::new(BTreeMap::new())),
            next_stream_id: 1,
            active_model: None,
            backend_report: None,
            activation_generation: 0,
            shutdown: true,
        })
    }
    pub fn with_engine_factory(
        config: InferenceConfig,
        lifecycle_generation: AuthorityGeneration,
        factory: Box<dyn InferenceEngineFactory>,
    ) -> Result<Self, InferenceError> {
        if lifecycle_generation.ordinal() == 0 {
            return Err(InferenceError::InvalidGeneration);
        }
        let worker_instance = NEXT_WORKER_INSTANCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1).filter(|next| *next <= i64::MAX as u64)
            })
            .map_err(|_| InferenceError::InvalidGeneration)?;
        let (sender, receiver) = mpsc::sync_channel(WORKER_QUEUE_CAPACITY);
        let active_streams = Arc::new(Mutex::new(BTreeMap::new()));
        let worker_streams = Arc::clone(&active_streams);
        let worker_config = config.clone();
        let worker = thread::Builder::new()
            .name("argentum-android-inference".into())
            .stack_size(INFERENCE_WORKER_STACK_BYTES)
            .spawn(move || worker_loop(worker_config, factory, receiver, worker_streams))
            .map_err(|_| InferenceError::WorkerUnavailable)?;

        Ok(Self {
            worker_instance,
            config,
            lifecycle_generation,
            sender,
            worker: Some(worker),
            active_streams,
            next_stream_id: 1,
            active_model: None,
            backend_report: None,
            activation_generation: 0,
            shutdown: false,
        })
    }

    #[cfg(target_os = "android")]
    pub fn new(
        config: InferenceConfig,
        lifecycle_generation: AuthorityGeneration,
    ) -> Result<Self, InferenceError> {
        Self::with_engine_factory(
            config,
            lifecycle_generation,
            Box::new(crate::android_engine::AndroidLlamaCppEngine::create),
        )
    }

    pub fn lifecycle_generation(&self) -> &AuthorityGeneration {
        &self.lifecycle_generation
    }

    /// Non-authority identity for generation-bound foreground-service messages.
    pub fn foreground_generation(&self) -> u64 {
        self.worker_instance
    }

    pub fn is_running(&self) -> bool {
        !self.shutdown && self.worker.is_some()
    }

    /// Reconstruct a stopped worker in the same owner cell. No model is loaded.
    pub fn restart_with_engine_factory(
        &mut self,
        factory: Box<dyn InferenceEngineFactory>,
    ) -> Result<(), InferenceError> {
        if !self.shutdown || self.worker.is_some() {
            return Err(InferenceError::WorkerUnavailable);
        }
        let replacement = Self::with_engine_factory(
            self.config.clone(),
            self.lifecycle_generation.clone(),
            factory,
        )?;
        *self = replacement;
        Ok(())
    }

    #[cfg(target_os = "android")]
    pub fn restart(&mut self) -> Result<(), InferenceError> {
        self.restart_with_engine_factory(Box::new(
            crate::android_engine::AndroidLlamaCppEngine::create,
        ))
    }

    pub fn model_directory(&self) -> &std::path::Path {
        self.config.app_private_model_dir()
    }

    pub fn active_model(&self) -> Option<&VerifiedModelDescriptor> {
        self.active_model.as_ref()
    }

    pub fn backend_report(&self) -> Option<&BackendReport> {
        self.backend_report.as_ref()
    }

    pub const fn activation_generation(&self) -> u64 {
        self.activation_generation
    }

    pub async fn prepare(
        &self,
        descriptor: VerifiedModelDescriptor,
    ) -> Result<PreparedModel, InferenceError> {
        self.ensure_live()?;
        let (reply, response) = oneshot::channel();
        self.send(WorkerCommand::Prepare {
            descriptor,
            lifecycle_generation: self.lifecycle_generation.clone(),
            reply,
        })?;
        let mut prepared = response
            .await
            .map_err(|_| InferenceError::WorkerUnavailable)??;
        prepared.bind_worker(self.worker_instance);
        Ok(prepared)
    }

    pub async fn activate(
        &mut self,
        prepared: PreparedModel,
        approval: &mut ActivationApproval,
    ) -> Result<BackendReport, InferenceError> {
        self.ensure_live()?;
        if !prepared.belongs_to_worker(self.worker_instance) {
            return Err(InferenceError::InvalidActivationApproval);
        }
        approval.consume(&prepared, &self.lifecycle_generation)?;
        self.cancel_streams();

        let (reply, response) = oneshot::channel();
        self.send(WorkerCommand::Load {
            prepared: prepared.clone(),
            reply,
        })?;
        let (result, worker_loaded) = response
            .await
            .map_err(|_| InferenceError::WorkerUnavailable)?;
        if !worker_loaded {
            self.active_model = None;
            self.backend_report = None;
            self.activation_generation = self
                .activation_generation
                .checked_add(1)
                .ok_or(InferenceError::InvalidGeneration)?;
        }
        let report = result?;
        self.activation_generation = self
            .activation_generation
            .checked_add(1)
            .ok_or(InferenceError::InvalidGeneration)?;
        self.active_model = Some(prepared.descriptor().clone());
        self.backend_report = Some(report.clone());
        Ok(report)
    }

    pub fn stream(&mut self, request: InferenceRequest) -> Result<StreamHandle, InferenceError> {
        self.ensure_live()?;
        request.validate(self.config.bounds())?;
        if self.active_model.is_none() {
            return Err(InferenceError::ModelNotLoaded);
        }
        let mut streams = self
            .active_streams
            .lock()
            .map_err(|_| InferenceError::WorkerUnavailable)?;
        if !streams.is_empty() {
            return Err(InferenceError::StreamBusy);
        }
        let stream_id = self.next_stream_id;
        self.next_stream_id = self
            .next_stream_id
            .checked_add(1)
            .ok_or(InferenceError::WorkerUnavailable)?;
        let cancellation = CancellationToken::new();
        streams.insert(stream_id, cancellation.clone());
        drop(streams);

        let (events, receiver) = event_channel::channel(self.config.bounds().stream_capacity());
        let terminal_fallback = Arc::new(Mutex::new(None));
        if let Err(error) = self.send(WorkerCommand::Stream {
            stream_id,
            request,
            cancellation: cancellation.clone(),
            events,
            terminal_fallback: Arc::clone(&terminal_fallback),
        }) {
            if let Ok(mut streams) = self.active_streams.lock() {
                streams.remove(&stream_id);
            }
            return Err(error);
        }
        Ok(StreamHandle {
            receiver,
            cancellation,
            terminal_fallback,
            terminal_seen: false,
        })
    }

    pub async fn account(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceInputReport, InferenceError> {
        self.ensure_live()?;
        request.validate(self.config.bounds())?;
        if self.active_model.is_none() {
            return Err(InferenceError::ModelNotLoaded);
        }
        let (reply, response) = oneshot::channel();
        self.send(WorkerCommand::Account { request, reply })?;
        response
            .await
            .map_err(|_| InferenceError::WorkerUnavailable)?
    }

    pub async fn unload(&mut self) -> Result<(), InferenceError> {
        self.ensure_live()?;
        let was_loaded = self.active_model.is_some();
        self.cancel_streams();
        let (reply, response) = oneshot::channel();
        self.send(WorkerCommand::Unload { reply })?;
        response
            .await
            .map_err(|_| InferenceError::WorkerUnavailable)??;
        self.active_model = None;
        self.backend_report = None;
        if was_loaded {
            self.activation_generation = self
                .activation_generation
                .checked_add(1)
                .ok_or(InferenceError::InvalidGeneration)?;
        }
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), InferenceError> {
        if self.shutdown {
            return Ok(());
        }
        self.cancel_streams();
        let (reply, response) = oneshot::channel();
        self.send(WorkerCommand::Shutdown { reply })?;
        let worker_result = response
            .await
            .map_err(|_| InferenceError::WorkerUnavailable)?;
        self.shutdown = true;
        self.active_model = None;
        self.backend_report = None;
        self.activation_generation = self.activation_generation.saturating_add(1);
        let join_result = self
            .worker
            .take()
            .ok_or(InferenceError::WorkerUnavailable)?
            .join()
            .map_err(|_| InferenceError::WorkerUnavailable);
        join_result?;
        worker_result
    }

    fn ensure_live(&self) -> Result<(), InferenceError> {
        if self.shutdown {
            Err(InferenceError::WorkerUnavailable)
        } else {
            Ok(())
        }
    }

    fn send(&self, command: WorkerCommand) -> Result<(), InferenceError> {
        self.sender
            .send(command)
            .map_err(|_| InferenceError::WorkerUnavailable)
    }

    fn cancel_streams(&self) {
        if let Ok(streams) = self.active_streams.lock() {
            for cancellation in streams.values() {
                cancellation.cancel();
            }
        }
    }
}

impl Drop for AndroidInferenceModule {
    fn drop(&mut self) {
        if self.shutdown {
            return;
        }
        self.cancel_streams();
        let (reply, _response) = oneshot::channel();
        let _ = self.sender.send(WorkerCommand::Shutdown { reply });
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.shutdown = true;
    }
}

enum WorkerCommand {
    Prepare {
        descriptor: VerifiedModelDescriptor,
        lifecycle_generation: AuthorityGeneration,
        reply: oneshot::Sender<Result<PreparedModel, InferenceError>>,
    },
    Load {
        prepared: PreparedModel,
        reply: oneshot::Sender<(Result<BackendReport, InferenceError>, bool)>,
    },
    Stream {
        stream_id: u64,
        request: InferenceRequest,
        cancellation: CancellationToken,
        events: event_channel::Sender<StreamEvent>,
        terminal_fallback: Arc<Mutex<Option<StreamEvent>>>,
    },
    Account {
        request: InferenceRequest,
        reply: oneshot::Sender<Result<InferenceInputReport, InferenceError>>,
    },
    Unload {
        reply: oneshot::Sender<Result<(), InferenceError>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<(), InferenceError>>,
    },
}

struct WorkerState {
    engine: Result<Box<dyn InferenceEngine>, EngineFailure>,
    loaded: bool,
}

fn worker_loop(
    config: InferenceConfig,
    factory: Box<dyn InferenceEngineFactory>,
    receiver: CommandReceiver<WorkerCommand>,
    active_streams: Arc<Mutex<BTreeMap<u64, CancellationToken>>>,
) {
    let mut state = WorkerState {
        engine: catch_unwind(AssertUnwindSafe(|| factory.create()))
            .unwrap_or_else(|_| Err(EngineFailure::new("engine initialization panicked"))),
        loaded: false,
    };

    while let Ok(command) = receiver.recv() {
        match command {
            WorkerCommand::Prepare {
                descriptor,
                lifecycle_generation,
                reply,
            } => {
                let result = prepare_model(&config, descriptor, lifecycle_generation);
                let _ = reply.send(result);
            }
            WorkerCommand::Load { prepared, reply } => {
                let result = load_model(&mut state, &prepared, config.bounds());
                let _ = reply.send((result, state.loaded));
            }
            WorkerCommand::Stream {
                stream_id,
                request,
                cancellation,
                events,
                terminal_fallback,
            } => {
                run_stream(
                    &mut state,
                    &request,
                    &cancellation,
                    &events,
                    &terminal_fallback,
                    config.bounds(),
                );
                if let Ok(mut streams) = active_streams.lock() {
                    streams.remove(&stream_id);
                }
            }
            WorkerCommand::Account { request, reply } => {
                let result = if !state.loaded {
                    Err(InferenceError::ModelNotLoaded)
                } else {
                    match state.engine.as_mut() {
                        Ok(engine) => catch_unwind(AssertUnwindSafe(|| engine.account(&request)))
                            .map_err(|_| {
                                InferenceError::Engine("engine accounting panicked".into())
                            })
                            .and_then(|result| result.map_err(engine_error)),
                        Err(error) => Err(InferenceError::Engine(error.message.clone())),
                    }
                };
                let _ = reply.send(result);
            }
            WorkerCommand::Unload { reply } => {
                let result = unload_model(&mut state);
                let _ = reply.send(result);
            }
            WorkerCommand::Shutdown { reply } => {
                let result = unload_model(&mut state);
                let _ = reply.send(result);
                break;
            }
        }
    }
}

fn load_model(
    state: &mut WorkerState,
    prepared: &PreparedModel,
    bounds: InferenceBounds,
) -> Result<BackendReport, InferenceError> {
    reverify_prepared(prepared, bounds)?;
    if state.loaded {
        unload_model(state)?;
    }
    let engine = state
        .engine
        .as_mut()
        .map_err(|error| InferenceError::Engine(error.message.clone()))?;
    let report = catch_unwind(AssertUnwindSafe(|| engine.load(prepared)))
        .map_err(|_| InferenceError::Engine("engine load panicked".into()))?
        .map_err(engine_error)?;
    if let Err(error) = reverify_prepared(prepared, bounds) {
        let _ = catch_unwind(AssertUnwindSafe(|| engine.unload()));
        return Err(error);
    }
    state.loaded = true;
    Ok(report)
}

fn unload_model(state: &mut WorkerState) -> Result<(), InferenceError> {
    if !state.loaded {
        return Ok(());
    }
    let engine = state
        .engine
        .as_mut()
        .map_err(|error| InferenceError::Engine(error.message.clone()))?;
    catch_unwind(AssertUnwindSafe(|| engine.unload()))
        .map_err(|_| InferenceError::Engine("engine unload panicked".into()))?
        .map_err(engine_error)?;
    state.loaded = false;
    Ok(())
}

fn run_stream(
    state: &mut WorkerState,
    request: &InferenceRequest,
    cancellation: &CancellationToken,
    events: &event_channel::Sender<StreamEvent>,
    terminal_fallback: &Arc<Mutex<Option<StreamEvent>>>,
    bounds: InferenceBounds,
) {
    let mut sequence = 0u64;
    let mut token_events = 0usize;
    let result = if !state.loaded {
        Err(EngineFailure::new("no model is loaded"))
    } else {
        match state.engine.as_mut() {
            Ok(engine) => catch_unwind(AssertUnwindSafe(|| {
                engine.stream(request, cancellation, &mut |fragment| {
                    if cancellation.is_cancelled() {
                        return Err(EngineFailure::new("stream cancelled"));
                    }
                    if fragment.is_empty() {
                        return Ok(());
                    }
                    if fragment.len() > bounds.max_event_bytes()
                        || token_events >= bounds.max_stream_events() - 1
                    {
                        return Err(EngineFailure::new("stream event bound exceeded"));
                    }
                    sequence = sequence
                        .checked_add(1)
                        .ok_or_else(|| EngineFailure::new("stream sequence exhausted"))?;
                    send_event(
                        events,
                        StreamEvent::new(sequence, StreamEventKind::Token(fragment.into())),
                        Some(cancellation),
                    )?;
                    token_events += 1;
                    Ok(())
                })
            }))
            .unwrap_or_else(|_| Err(EngineFailure::new("engine stream panicked"))),
            Err(error) => Err(error.clone()),
        }
    };

    sequence = sequence.saturating_add(1);
    let terminal =
        if cancellation.is_cancelled() || matches!(result, Ok(EngineStreamOutcome::Cancelled)) {
            StreamEventKind::Cancelled
        } else {
            match result {
                Ok(EngineStreamOutcome::Completed) => StreamEventKind::Completed,
                Ok(EngineStreamOutcome::Cancelled) => StreamEventKind::Cancelled,
                Err(error) => StreamEventKind::Failed(error.message),
            }
        };
    let terminal = StreamEvent::new(sequence, terminal);
    if let Ok(mut fallback) = terminal_fallback.lock() {
        *fallback = Some(terminal.clone());
    }
    let _ = events.try_send(terminal);
}

fn send_event(
    sender: &event_channel::Sender<StreamEvent>,
    event: StreamEvent,
    cancellation: Option<&CancellationToken>,
) -> Result<(), EngineFailure> {
    let mut pending = event;
    loop {
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err(EngineFailure::new("stream cancelled"));
        }
        match sender.try_send(pending) {
            Ok(()) => return Ok(()),
            Err(event_channel::error::TrySendError::Closed(_)) => {
                return Err(EngineFailure::new("stream receiver closed"));
            }
            Err(event_channel::error::TrySendError::Full(event)) => {
                pending = event;
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn engine_error(error: EngineFailure) -> InferenceError {
    InferenceError::Engine(error.message)
}

fn bounded_message(message: &str) -> String {
    let mut output = String::with_capacity(message.len().min(256));
    for character in message.chars() {
        if output.len() >= 256 {
            break;
        }
        let character = if character.is_ascii_graphic() || character == ' ' {
            character
        } else {
            '?'
        };
        if output.len() + character.len_utf8() <= 256 {
            output.push(character);
        }
    }
    if output.is_empty() {
        "engine failure".into()
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{Arc, Mutex},
        thread::ThreadId,
        time::Duration,
    };

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::{BackendKind, ChatMessage, ChatRole, StreamEventKind};

    fn generation(ordinal: u64) -> AuthorityGeneration {
        let (_, _, mut generation) =
            argentum_security::host_authority_channel().expect("authority channel");
        while generation.ordinal() < ordinal {
            generation = generation.successor().expect("authority successor");
        }
        assert_eq!(generation.ordinal(), ordinal);
        generation
    }

    #[derive(Clone)]
    struct FakeControl {
        actions: Arc<Mutex<Vec<String>>>,
        threads: Arc<Mutex<Vec<ThreadId>>>,
        fragments: Arc<Mutex<Vec<String>>>,
        fail_load: Arc<Mutex<Option<String>>>,
        tamper_on_load: Arc<Mutex<Option<String>>>,
        delay: Duration,
    }

    impl FakeControl {
        fn new(fragments: &[&str], delay: Duration) -> Self {
            Self {
                actions: Arc::new(Mutex::new(Vec::new())),
                threads: Arc::new(Mutex::new(Vec::new())),
                fragments: Arc::new(Mutex::new(
                    fragments.iter().map(|value| (*value).into()).collect(),
                )),
                fail_load: Arc::new(Mutex::new(None)),
                tamper_on_load: Arc::new(Mutex::new(None)),
                delay,
            }
        }

        fn fail_load(&self, file_name: &str) {
            *self.fail_load.lock().expect("load failure lock") = Some(file_name.into());
        }

        fn tamper_on_load(&self, file_name: &str) {
            *self.tamper_on_load.lock().expect("tamper lock") = Some(file_name.into());
        }

        fn factory(&self) -> Box<dyn InferenceEngineFactory> {
            let control = self.clone();
            Box::new(move || {
                control.record("create");
                Ok(Box::new(FakeEngine { control }) as Box<dyn InferenceEngine>)
            })
        }

        fn record(&self, action: impl Into<String>) {
            self.actions
                .lock()
                .expect("actions lock")
                .push(action.into());
            self.threads
                .lock()
                .expect("threads lock")
                .push(std::thread::current().id());
        }

        fn actions(&self) -> Vec<String> {
            self.actions.lock().expect("actions lock").clone()
        }
    }

    struct FakeEngine {
        control: FakeControl,
    }

    impl InferenceEngine for FakeEngine {
        fn load(&mut self, model: &PreparedModel) -> Result<BackendReport, EngineFailure> {
            self.control
                .record(format!("load:{}", model.descriptor().file_name()));
            if self
                .control
                .fail_load
                .lock()
                .expect("load failure lock")
                .as_deref()
                == Some(model.descriptor().file_name())
            {
                return Err(EngineFailure::new("fixture load failure"));
            }
            if self
                .control
                .tamper_on_load
                .lock()
                .expect("tamper lock")
                .as_deref()
                == Some(model.descriptor().file_name())
            {
                fs::write(model.canonical_path(), b"tampered during load")
                    .expect("tamper model during load");
            }
            BackendReport::new(BackendKind::Test, "fake-engine")
                .map_err(|error| EngineFailure::new(error.to_string()))
        }

        fn stream(
            &mut self,
            _request: &InferenceRequest,
            cancellation: &CancellationToken,
            emit: &mut dyn FnMut(&str) -> Result<(), EngineFailure>,
        ) -> Result<EngineStreamOutcome, EngineFailure> {
            self.control.record("stream");
            let fragments = self
                .control
                .fragments
                .lock()
                .expect("fragments lock")
                .clone();
            for fragment in fragments {
                if cancellation.is_cancelled() {
                    return Ok(EngineStreamOutcome::Cancelled);
                }
                emit(&fragment)?;
                if !self.control.delay.is_zero() {
                    std::thread::sleep(self.control.delay);
                }
            }
            Ok(EngineStreamOutcome::Completed)
        }

        fn unload(&mut self) -> Result<(), EngineFailure> {
            self.control.record("unload");
            Ok(())
        }
    }

    impl Drop for FakeEngine {
        fn drop(&mut self) {
            self.control.record("drop");
        }
    }

    struct Fixture {
        directory: TempDir,
        config: InferenceConfig,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().expect("temporary directory");
            let config = InferenceConfig::new(directory.path(), InferenceBounds::default())
                .expect("valid config");
            Self { directory, config }
        }

        fn descriptor(&self, file_name: &str, content: &[u8]) -> VerifiedModelDescriptor {
            fs::write(self.directory.path().join(file_name), content).expect("write model");
            let sha = Sha256::digest(content);
            VerifiedModelDescriptor::new(
                file_name,
                file_name,
                content.len() as u64,
                &format!("{sha:x}"),
            )
            .expect("valid descriptor")
        }
    }

    fn request() -> InferenceRequest {
        InferenceRequest::new(vec![ChatMessage::new(ChatRole::User, "hello")])
    }

    async fn load(
        module: &mut AndroidInferenceModule,
        descriptor: VerifiedModelDescriptor,
    ) -> PreparedModel {
        let prepared = module.prepare(descriptor).await.expect("prepare model");
        let mut approval = ActivationApproval::bind(&prepared);
        module
            .activate(prepared.clone(), &mut approval)
            .await
            .expect("activate model");
        assert!(!approval.is_unused());
        prepared
    }

    #[tokio::test]
    async fn prepare_enforces_leaf_regular_size_hash_and_containment() {
        let fixture = Fixture::new();
        let control = FakeControl::new(&[], Duration::ZERO);
        let mut module = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(7),
            control.factory(),
        )
        .expect("module");
        let descriptor = fixture.descriptor("model.gguf", b"verified model bytes");
        let prepared = module
            .prepare(descriptor.clone())
            .await
            .expect("verified preparation");
        assert_eq!(prepared.lifecycle_generation().ordinal(), 7);
        assert_eq!(prepared.descriptor(), &descriptor);
        let canonical_model_dir = fixture
            .directory
            .path()
            .canonicalize()
            .expect("canonical model directory");
        assert_eq!(
            prepared.canonical_path().parent(),
            Some(canonical_model_dir.as_path())
        );

        let nested =
            VerifiedModelDescriptor::new("nested", "nested/model.gguf", 1, &"00".repeat(32));
        assert_eq!(nested, Err(InferenceError::InvalidModelDescriptor));

        let wrong_size =
            VerifiedModelDescriptor::new("wrong size", "model.gguf", 1, &descriptor.sha256_hex())
                .expect("descriptor");
        assert!(matches!(
            module.prepare(wrong_size).await,
            Err(InferenceError::ModelSizeMismatch)
        ));

        let wrong_hash = VerifiedModelDescriptor::new(
            "wrong hash",
            "model.gguf",
            descriptor.exact_size(),
            &"00".repeat(32),
        )
        .expect("descriptor");
        assert!(matches!(
            module.prepare(wrong_hash).await,
            Err(InferenceError::ModelHashMismatch)
        ));

        fs::create_dir(fixture.directory.path().join("directory.gguf")).expect("create directory");
        let directory_descriptor =
            VerifiedModelDescriptor::new("directory", "directory.gguf", 1, &"00".repeat(32))
                .expect("descriptor");
        assert!(matches!(
            module.prepare(directory_descriptor).await,
            Err(InferenceError::ModelNotRegularFile)
        ));
        module.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn preparation_from_another_worker_is_rejected_even_with_same_authority() {
        let fixture = Fixture::new();
        let authority = generation(7);
        let control = FakeControl::new(&[], Duration::ZERO);
        let mut first = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            authority.clone(),
            control.factory(),
        )
        .unwrap();
        let mut second = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            authority,
            control.factory(),
        )
        .unwrap();
        let prepared = first
            .prepare(fixture.descriptor("worker.gguf", b"model"))
            .await
            .unwrap();
        let mut approval = ActivationApproval::bind(&prepared);
        let result = second.activate(prepared, &mut approval).await;
        first.shutdown().await.unwrap();
        second.shutdown().await.unwrap();
        assert!(matches!(
            result,
            Err(InferenceError::InvalidActivationApproval)
        ));
        assert!(!control
            .actions()
            .iter()
            .any(|action| action.starts_with("load:")));
    }

    #[tokio::test]
    async fn stopped_worker_restarts_without_loading_or_reusing_prepared_actions() {
        let fixture = Fixture::new();
        let control = FakeControl::new(&[], Duration::ZERO);
        let mut module = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(7),
            control.factory(),
        )
        .unwrap();
        assert!(module
            .restart_with_engine_factory(control.factory())
            .is_err());
        let descriptor = fixture.descriptor("restart.gguf", b"model");
        let old = module.prepare(descriptor.clone()).await.unwrap();
        let mut old_approval = ActivationApproval::bind(&old);
        module.shutdown().await.unwrap();
        module
            .restart_with_engine_factory(control.factory())
            .unwrap();
        assert!(module.active_model().is_none());
        let fresh = module.prepare(descriptor).await.unwrap();
        assert_ne!(old.plan_digest(), fresh.plan_digest());
        assert!(matches!(
            module.activate(old, &mut old_approval).await,
            Err(InferenceError::InvalidActivationApproval)
        ));
        assert!(!control
            .actions()
            .iter()
            .any(|action| action.starts_with("load:")));
        let mut approval = ActivationApproval::bind(&fresh);
        module.activate(fresh, &mut approval).await.unwrap();
        assert!(module.active_model().is_some());
        module.shutdown().await.unwrap();
        let actions = control.actions();
        assert!(
            actions.iter().position(|a| a == "drop").unwrap()
                < actions.iter().rposition(|a| a == "create").unwrap()
        );
    }

    #[tokio::test]
    async fn dormant_configuration_does_not_admit_work_until_enabled() {
        let fixture = Fixture::new();
        let control = FakeControl::new(&[], Duration::ZERO);
        let mut module =
            AndroidInferenceModule::stopped(fixture.config.clone(), generation(7)).unwrap();
        let descriptor = fixture.descriptor("dormant.gguf", b"model");
        assert!(matches!(
            module.prepare(descriptor.clone()).await,
            Err(InferenceError::WorkerUnavailable)
        ));
        assert_eq!(module.foreground_generation(), 0);
        module
            .restart_with_engine_factory(control.factory())
            .unwrap();
        module.prepare(descriptor).await.unwrap();
        assert_eq!(control.actions(), vec!["create"]);
        module.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn approval_is_one_use_and_bound_to_plan_and_generation() {
        let fixture = Fixture::new();
        let control = FakeControl::new(&[], Duration::ZERO);
        let mut module = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(9),
            control.factory(),
        )
        .expect("module");
        let first = module
            .prepare(fixture.descriptor("first.gguf", b"first"))
            .await
            .expect("first prepared");
        let second = module
            .prepare(fixture.descriptor("second.gguf", b"second"))
            .await
            .expect("second prepared");
        let mut approval = ActivationApproval::bind(&first);
        assert_eq!(
            module.activate(second, &mut approval).await,
            Err(InferenceError::InvalidActivationApproval)
        );
        assert!(approval.is_unused());
        module
            .activate(first.clone(), &mut approval)
            .await
            .expect("approved activation");
        assert!(!approval.is_unused());
        assert_eq!(
            module.activate(first, &mut approval).await,
            Err(InferenceError::InvalidActivationApproval)
        );
        module.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn independent_same_ordinal_generation_cannot_activate_prepared_model() {
        let fixture = Fixture::new();
        let first_generation = generation(1);
        let second_generation = generation(1);
        assert_ne!(first_generation, second_generation);

        let first_control = FakeControl::new(&[], Duration::ZERO);
        let mut first = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            first_generation,
            first_control.factory(),
        )
        .expect("first module");
        let prepared = first
            .prepare(fixture.descriptor("model.gguf", b"model"))
            .await
            .expect("prepared model");

        let second_control = FakeControl::new(&[], Duration::ZERO);
        let mut second = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            second_generation,
            second_control.factory(),
        )
        .expect("second module");
        let mut approval = ActivationApproval::bind(&prepared);
        assert_eq!(
            second.activate(prepared, &mut approval).await,
            Err(InferenceError::InvalidActivationApproval)
        );
        assert!(approval.is_unused());
        first.shutdown().await.expect("first shutdown");
        second.shutdown().await.expect("second shutdown");
    }

    #[tokio::test]
    async fn activation_rechecks_integrity_and_rejects_another_generation() {
        let fixture = Fixture::new();
        let descriptor = fixture.descriptor("model.gguf", b"original");
        let first_control = FakeControl::new(&[], Duration::ZERO);
        let mut first = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(17),
            first_control.factory(),
        )
        .expect("first module");
        let prepared = first
            .prepare(descriptor.clone())
            .await
            .expect("prepared model");

        let second_control = FakeControl::new(&[], Duration::ZERO);
        let mut second = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(18),
            second_control.factory(),
        )
        .expect("second module");
        let mut wrong_generation = ActivationApproval::bind(&prepared);
        assert_eq!(
            second
                .activate(prepared.clone(), &mut wrong_generation)
                .await,
            Err(InferenceError::InvalidActivationApproval)
        );
        assert!(wrong_generation.is_unused());

        fs::write(fixture.directory.path().join("model.gguf"), b"tampered").expect("tamper model");
        let mut approved = ActivationApproval::bind(&prepared);
        assert_eq!(
            first.activate(prepared, &mut approved).await,
            Err(InferenceError::ModelHashMismatch)
        );
        assert!(!approved.is_unused());
        assert_eq!(first_control.actions(), vec!["create"]);
        first.shutdown().await.expect("first shutdown");
        second.shutdown().await.expect("second shutdown");
    }

    #[tokio::test]
    async fn stream_is_bounded_ordered_and_terminal() {
        let fixture = Fixture::new();
        let control = FakeControl::new(&["one", "two", "three"], Duration::ZERO);
        let mut module = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(3),
            control.factory(),
        )
        .expect("module");
        load(&mut module, fixture.descriptor("model.gguf", b"model")).await;
        let report = module.backend_report().expect("backend report");
        assert_eq!(report.kind(), BackendKind::Test);
        assert_eq!(report.context_tokens(), 4096);
        assert_eq!(report.max_generated_tokens(), 512);

        let mut stream = module.stream(request()).expect("stream");
        let mut events = Vec::new();
        while let Some(event) = stream.recv().await {
            let terminal = !matches!(event.kind(), StreamEventKind::Token(_));
            events.push(event);
            if terminal {
                break;
            }
        }
        assert_eq!(events.len(), 4);
        assert_eq!(
            events.iter().map(StreamEvent::sequence).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(events[0].kind(), &StreamEventKind::Token("one".into()));
        assert_eq!(events[1].kind(), &StreamEventKind::Token("two".into()));
        assert_eq!(events[2].kind(), &StreamEventKind::Token("three".into()));
        assert_eq!(events[3].kind(), &StreamEventKind::Completed);
        assert!(stream.recv().await.is_none());
        module.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn stream_event_limit_fails_closed_at_the_bound() {
        let fixture = Fixture::new();
        let bounds = InferenceBounds::new(1024, 1024, 3, 1024, 3).expect("tight bounds");
        let config = InferenceConfig::new(fixture.directory.path(), bounds).expect("config");
        let control = FakeControl::new(&["one", "two", "three"], Duration::ZERO);
        let mut module =
            AndroidInferenceModule::with_engine_factory(config, generation(19), control.factory())
                .expect("module");
        load(&mut module, fixture.descriptor("model.gguf", b"model")).await;
        let mut stream = module.stream(request()).expect("stream");
        let first = stream.recv().await.expect("first");
        let second = stream.recv().await.expect("second");
        let terminal = stream.recv().await.expect("terminal");
        assert_eq!(first.sequence(), 1);
        assert_eq!(second.sequence(), 2);
        assert_eq!(terminal.sequence(), 3);
        assert!(matches!(terminal.kind(), StreamEventKind::Failed(_)));
        assert!(stream.recv().await.is_none());
        module.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn cancellation_suppresses_buffered_tokens_and_all_late_events() {
        let fixture = Fixture::new();
        let fragments = vec!["token"; 100];
        let control = FakeControl::new(&fragments, Duration::from_millis(3));
        let mut module = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(5),
            control.factory(),
        )
        .expect("module");
        load(&mut module, fixture.descriptor("model.gguf", b"model")).await;
        let mut stream = module.stream(request()).expect("stream");
        let first = stream.recv().await.expect("first event");
        assert!(matches!(first.kind(), StreamEventKind::Token(_)));
        stream.cancel();
        let terminal = tokio::time::timeout(Duration::from_secs(2), stream.recv())
            .await
            .expect("cancellation deadline")
            .expect("cancelled event");
        assert_eq!(terminal.kind(), &StreamEventKind::Cancelled);
        assert!(stream.recv().await.is_none());
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(stream.recv().await.is_none());
        module.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn replacement_unloads_before_loading_and_explicit_unload_clears_state() {
        let fixture = Fixture::new();
        let control = FakeControl::new(&[], Duration::ZERO);
        let mut module = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(11),
            control.factory(),
        )
        .expect("module");
        load(&mut module, fixture.descriptor("first.gguf", b"first")).await;
        load(&mut module, fixture.descriptor("second.gguf", b"second")).await;
        assert_eq!(
            control.actions(),
            vec!["create", "load:first.gguf", "unload", "load:second.gguf"]
        );
        module.unload().await.expect("explicit unload");
        assert!(module.active_model().is_none());
        assert!(module.backend_report().is_none());
        assert!(matches!(
            module.stream(request()),
            Err(InferenceError::ModelNotLoaded)
        ));
        module.shutdown().await.expect("shutdown");
        assert_eq!(
            control.actions(),
            vec![
                "create",
                "load:first.gguf",
                "unload",
                "load:second.gguf",
                "unload",
                "drop"
            ]
        );
    }

    #[tokio::test]
    async fn failed_replacement_does_not_publish_a_stale_active_model() {
        let fixture = Fixture::new();
        let control = FakeControl::new(&[], Duration::ZERO);
        let mut module = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(12),
            control.factory(),
        )
        .expect("module");
        load(&mut module, fixture.descriptor("first.gguf", b"first")).await;
        control.fail_load("second.gguf");
        let prepared = module
            .prepare(fixture.descriptor("second.gguf", b"second"))
            .await
            .expect("prepared replacement");
        let mut approval = ActivationApproval::bind(&prepared);

        assert!(matches!(
            module.activate(prepared, &mut approval).await,
            Err(InferenceError::Engine(_))
        ));
        assert!(module.active_model().is_none());
        assert!(module.backend_report().is_none());
        assert!(matches!(
            module.stream(request()),
            Err(InferenceError::ModelNotLoaded)
        ));
        module.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn activation_rechecks_integrity_after_engine_opens_the_model() {
        let fixture = Fixture::new();
        let control = FakeControl::new(&[], Duration::ZERO);
        control.tamper_on_load("model.gguf");
        let mut module = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(20),
            control.factory(),
        )
        .expect("module");
        let prepared = module
            .prepare(fixture.descriptor("model.gguf", b"original model"))
            .await
            .expect("prepared model");
        let mut approval = ActivationApproval::bind(&prepared);

        assert_eq!(
            module.activate(prepared, &mut approval).await,
            Err(InferenceError::ModelSizeMismatch)
        );
        assert!(module.active_model().is_none());
        assert!(module.backend_report().is_none());
        assert_eq!(
            control.actions(),
            vec!["create", "load:model.gguf", "unload"]
        );
        module.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn engine_creation_load_stream_unload_and_drop_stay_on_one_worker() {
        let fixture = Fixture::new();
        let control = FakeControl::new(&["answer"], Duration::ZERO);
        let caller = std::thread::current().id();
        let mut module = AndroidInferenceModule::with_engine_factory(
            fixture.config.clone(),
            generation(13),
            control.factory(),
        )
        .expect("module");
        load(&mut module, fixture.descriptor("model.gguf", b"model")).await;
        let mut stream = module.stream(request()).expect("stream");
        while let Some(event) = stream.recv().await {
            if !matches!(event.kind(), StreamEventKind::Token(_)) {
                break;
            }
        }
        module.shutdown().await.expect("shutdown");
        let threads = control.threads.lock().expect("threads lock").clone();
        assert!(!threads.is_empty());
        assert!(threads.iter().all(|thread| *thread == threads[0]));
        assert_ne!(threads[0], caller);
        assert_eq!(
            control.actions(),
            vec!["create", "load:model.gguf", "stream", "unload", "drop"]
        );
    }
}
