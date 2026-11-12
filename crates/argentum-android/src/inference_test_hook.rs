use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    time::Duration,
};

use argentum_android_inference::{
    ActivationApproval, AndroidInferenceModule, BackendKind, ChatMessage, ChatRole,
    InferenceBounds, InferenceConfig, InferenceRequest, StreamEventKind, VerifiedModelDescriptor,
};
use argentum_providers::{android_local_model_offers, download_local_model, ProviderError};
use argentum_security::host_authority_channel;
use jni::{
    objects::{JClass, JString},
    Env, EnvUnowned,
};

const DEVICE_EMULATOR: &str = "emulator";
const DEVICE_PHYSICAL_ARM64: &str = "physical-arm64";
const PHASE_PRIMARY: &str = "primary";
const PHASE_RESTART: &str = "restart";
const EVENT_TIMEOUT: Duration = Duration::from_secs(180);
const TEST_WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_argentum_bridge_ArgentumInferenceTestHook_runNative<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    files_dir: JString<'caller>,
    device_class: JString<'caller>,
    apk_sha256: JString<'caller>,
    model_sha256: JString<'caller>,
    phase: JString<'caller>,
) -> JString<'caller> {
    unowned_env
        .with_env(|env| -> Result<_, jni::errors::Error> {
            let inputs = read_inputs(
                env,
                files_dir,
                device_class,
                apk_sha256,
                model_sha256,
                phase,
            );
            let output = match inputs {
                Ok(inputs) => match catch_unwind(AssertUnwindSafe(|| run(inputs))) {
                    Ok(Ok(output)) => output,
                    Ok(Err(stage)) => failure_evidence(stage),
                    Err(_) => failure_evidence("panic"),
                },
                Err(stage) => failure_evidence(stage),
            };
            JString::from_str(env, output)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

struct Inputs {
    files_dir: PathBuf,
    device_class: String,
    apk_sha256: String,
    model_sha256: String,
    phase: String,
}

fn read_inputs(
    env: &mut Env<'_>,
    files_dir: JString<'_>,
    device_class: JString<'_>,
    apk_sha256: JString<'_>,
    model_sha256: JString<'_>,
    phase: JString<'_>,
) -> Result<Inputs, &'static str> {
    let files_dir = read_string(env, &files_dir)?;
    let device_class = read_string(env, &device_class)?;
    let apk_sha256 = read_string(env, &apk_sha256)?;
    let model_sha256 = read_string(env, &model_sha256)?;
    let phase = read_string(env, &phase)?;
    if files_dir.is_empty() || files_dir.contains('\0') {
        return Err("files-dir");
    }
    if !matches!(
        device_class.as_str(),
        DEVICE_EMULATOR | DEVICE_PHYSICAL_ARM64
    ) {
        return Err("device-class");
    }
    if !valid_sha256(&apk_sha256) || !valid_sha256(&model_sha256) {
        return Err("artifact-hash");
    }
    if !matches!(phase.as_str(), PHASE_PRIMARY | PHASE_RESTART) {
        return Err("phase");
    }
    Ok(Inputs {
        files_dir: PathBuf::from(files_dir),
        device_class,
        apk_sha256: apk_sha256.to_ascii_uppercase(),
        model_sha256: model_sha256.to_ascii_uppercase(),
        phase,
    })
}

fn read_string(env: &mut Env<'_>, value: &JString<'_>) -> Result<String, &'static str> {
    value.try_to_string(env).map_err(|_| "jni-input")
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn run(inputs: Inputs) -> Result<String, &'static str> {
    std::thread::Builder::new()
        .name("argentum-inference-test".into())
        .stack_size(TEST_WORKER_STACK_BYTES)
        .spawn(move || run_on_worker(inputs))
        .map_err(|_| "worker-spawn")?
        .join()
        .map_err(|_| "worker-panic")?
}

fn run_on_worker(inputs: Inputs) -> Result<String, &'static str> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "runtime")?;
    runtime.block_on(run_async(inputs))
}

async fn run_async(inputs: Inputs) -> Result<String, &'static str> {
    let offer = *android_local_model_offers().first().ok_or("model-offer")?;
    if !offer.sha256.eq_ignore_ascii_case(&inputs.model_sha256) {
        return Err("model-hash-binding");
    }
    let private_root =
        std::fs::canonicalize(&inputs.files_dir).map_err(|_| "files-dir-canonicalize")?;
    let model_dir = private_root.join("models");
    tokio::fs::create_dir_all(&model_dir)
        .await
        .map_err(|_| "model-directory")?;
    download_local_model(offer, &model_dir)
        .await
        .map_err(model_download_stage)?;

    let descriptor =
        VerifiedModelDescriptor::new(offer.label, offer.filename, offer.size_bytes, offer.sha256)
            .map_err(|_| "model-descriptor")?;
    let (_, _, generation) = host_authority_channel().map_err(|_| "authority-generation")?;
    let config = InferenceConfig::new(&model_dir, InferenceBounds::default())
        .map_err(|_| "inference-config")?;
    let mut module =
        AndroidInferenceModule::new(config, generation).map_err(|_| "module-create")?;
    let prepared = module
        .prepare(descriptor.clone())
        .await
        .map_err(|_| "model-prepare")?;
    let mut approval = ActivationApproval::bind(&prepared);
    let backend = module
        .activate(prepared, &mut approval)
        .await
        .map_err(|_| "model-load")?;

    let backend_name = match backend.kind() {
        BackendKind::Vulkan => "vulkan",
        BackendKind::Cpu => "cpu",
        BackendKind::Test => return Err("test-backend"),
    };
    let backend_value = format!("{} {}", backend_name, backend.runtime());

    if inputs.phase == PHASE_RESTART {
        module.unload().await.map_err(|_| "restart-unload")?;
        module.shutdown().await.map_err(|_| "restart-shutdown")?;
        return Ok(evidence_lines(&[
            ("protocol", "1".into()),
            ("status", "pass".into()),
            ("device_class", inputs.device_class),
            ("apk_sha256", inputs.apk_sha256),
            ("model_sha256", inputs.model_sha256),
            ("load", "pass".into()),
            ("unload", "pass".into()),
            ("restart", "pass".into()),
            ("backend", backend_value),
        ]));
    }

    let request = InferenceRequest::new(vec![ChatMessage::new(
        ChatRole::User,
        "Count upward from one using one integer per line. Continue until stopped.",
    )]);
    let mut stream = module.stream(request).map_err(|_| "stream-start")?;
    let mut sequences = Vec::new();
    let mut token_count = 0usize;
    while token_count < 2 {
        let event = tokio::time::timeout(EVENT_TIMEOUT, stream.recv())
            .await
            .map_err(|_| "stream-timeout")?
            .ok_or("stream-ended")?;
        if sequences
            .last()
            .is_some_and(|last| event.sequence() <= *last)
        {
            return Err("stream-order");
        }
        sequences.push(event.sequence());
        match event.kind() {
            StreamEventKind::Token(_) => token_count += 1,
            StreamEventKind::Completed => return Err("stream-completed-before-cancel"),
            StreamEventKind::Cancelled => return Err("stream-cancelled-before-request"),
            StreamEventKind::Failed(_) => return Err("stream-failed"),
        }
    }
    stream.cancel();
    let terminal = tokio::time::timeout(EVENT_TIMEOUT, stream.recv())
        .await
        .map_err(|_| "cancel-timeout")?
        .ok_or("cancel-terminal-missing")?;
    if sequences
        .last()
        .is_some_and(|last| terminal.sequence() <= *last)
    {
        return Err("cancel-order");
    }
    sequences.push(terminal.sequence());
    if !matches!(terminal.kind(), StreamEventKind::Cancelled) {
        return Err("cancel-terminal");
    }
    if stream.recv().await.is_some() {
        return Err("late-event");
    }

    let replacement = module
        .prepare(descriptor)
        .await
        .map_err(|_| "replacement-prepare")?;
    let mut replacement_approval = ActivationApproval::bind(&replacement);
    module
        .activate(replacement, &mut replacement_approval)
        .await
        .map_err(|_| "replacement-load")?;
    module.unload().await.map_err(|_| "unload")?;
    if module.active_model().is_some() || module.backend_report().is_some() {
        return Err("unload-state");
    }
    module.shutdown().await.map_err(|_| "shutdown")?;

    Ok(evidence_lines(&[
        ("protocol", "1".into()),
        ("status", "pass".into()),
        ("device_class", inputs.device_class),
        ("apk_sha256", inputs.apk_sha256),
        ("model_sha256", inputs.model_sha256),
        ("load", "pass".into()),
        (
            "stream_sequence",
            sequences
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ),
        ("cancel", "pass".into()),
        ("no_late_event", "pass".into()),
        ("replacement", "pass".into()),
        ("unload", "pass".into()),
        ("backend", backend_value),
    ]))
}

fn model_download_stage(error: ProviderError) -> &'static str {
    match error {
        ProviderError::LocalModelDownloadDestinationInvalid => "model-download-destination",
        ProviderError::LocalModelDownloadDeadline { .. } => "model-download-deadline",
        ProviderError::LocalModelDownloadTooLarge { .. } => "model-download-size",
        ProviderError::LocalModelIntegrityMismatch => "model-download-integrity",
        ProviderError::LocalModelDownloadFailed => "model-download-transfer",
        _ => "model-download-unexpected",
    }
}

fn evidence_lines(values: &[(&str, String)]) -> String {
    values
        .iter()
        .map(|(key, value)| format!("ARGENTUM_M3 {key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn failure_evidence(stage: &str) -> String {
    evidence_lines(&[
        ("protocol", "1".into()),
        ("status", "fail".into()),
        ("failure_stage", stage.into()),
    ])
}
