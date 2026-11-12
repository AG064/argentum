use std::{
    ffi::OsStr,
    fmt,
    fs::File,
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
};

use argentum_security::AuthorityGeneration;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const ANDROID_CONTEXT_TOKENS: u32 = 4096;
pub const ANDROID_MAX_GENERATED_TOKENS: usize = 512;

const HARD_MAX_MODEL_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const HARD_MAX_PROMPT_BYTES: usize = 256 * 1024;
const HARD_MAX_STREAM_EVENTS: usize = ANDROID_MAX_GENERATED_TOKENS + 1;
const HARD_MAX_EVENT_BYTES: usize = 64 * 1024;
const HARD_MAX_STREAM_CAPACITY: usize = 128;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum InferenceError {
    #[error("the app-private model directory is invalid")]
    InvalidModelDirectory,
    #[error("inference bounds are invalid")]
    InvalidBounds,
    #[error("the lifecycle generation must be nonzero")]
    InvalidGeneration,
    #[error("the model descriptor is invalid")]
    InvalidModelDescriptor,
    #[error("the model path must be one file-name component")]
    InvalidModelLeaf,
    #[error("the model path escapes the app-private model directory")]
    ModelOutsidePrivateDirectory,
    #[error("the model file is unavailable")]
    ModelUnavailable,
    #[error("the model path is not a regular file")]
    ModelNotRegularFile,
    #[error("the model exceeds the configured size bound")]
    ModelTooLarge,
    #[error("the model size does not match its descriptor")]
    ModelSizeMismatch,
    #[error("the model SHA-256 does not match its descriptor")]
    ModelHashMismatch,
    #[error("the activation approval is invalid, stale, or already used")]
    InvalidActivationApproval,
    #[error("no model is loaded")]
    ModelNotLoaded,
    #[error("an inference stream is already active")]
    StreamBusy,
    #[error("the inference request is invalid or exceeds its bounds")]
    InvalidRequest,
    #[error("the inference worker is unavailable")]
    WorkerUnavailable,
    #[error("the inference worker failed: {0}")]
    Engine(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InferenceBounds {
    max_model_bytes: u64,
    max_prompt_bytes: usize,
    max_stream_events: usize,
    max_event_bytes: usize,
    stream_capacity: usize,
}

impl Default for InferenceBounds {
    fn default() -> Self {
        Self {
            max_model_bytes: 1024 * 1024 * 1024,
            max_prompt_bytes: 64 * 1024,
            max_stream_events: HARD_MAX_STREAM_EVENTS,
            max_event_bytes: 16 * 1024,
            stream_capacity: 32,
        }
    }
}

impl InferenceBounds {
    pub fn new(
        max_model_bytes: u64,
        max_prompt_bytes: usize,
        max_stream_events: usize,
        max_event_bytes: usize,
        stream_capacity: usize,
    ) -> Result<Self, InferenceError> {
        let bounds = Self {
            max_model_bytes,
            max_prompt_bytes,
            max_stream_events,
            max_event_bytes,
            stream_capacity,
        };
        bounds.validate()?;
        Ok(bounds)
    }

    pub const fn max_model_bytes(self) -> u64 {
        self.max_model_bytes
    }

    pub const fn max_prompt_bytes(self) -> usize {
        self.max_prompt_bytes
    }

    pub const fn max_stream_events(self) -> usize {
        self.max_stream_events
    }

    pub const fn max_event_bytes(self) -> usize {
        self.max_event_bytes
    }

    pub const fn stream_capacity(self) -> usize {
        self.stream_capacity
    }

    fn validate(self) -> Result<(), InferenceError> {
        if self.max_model_bytes == 0
            || self.max_model_bytes > HARD_MAX_MODEL_BYTES
            || self.max_prompt_bytes == 0
            || self.max_prompt_bytes > HARD_MAX_PROMPT_BYTES
            || self.max_stream_events < 2
            || self.max_stream_events > HARD_MAX_STREAM_EVENTS
            || self.max_event_bytes == 0
            || self.max_event_bytes > HARD_MAX_EVENT_BYTES
            || self.stream_capacity == 0
            || self.stream_capacity > HARD_MAX_STREAM_CAPACITY
        {
            return Err(InferenceError::InvalidBounds);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceConfig {
    app_private_model_dir: PathBuf,
    bounds: InferenceBounds,
}

impl InferenceConfig {
    pub fn new(
        app_private_model_dir: impl Into<PathBuf>,
        bounds: InferenceBounds,
    ) -> Result<Self, InferenceError> {
        let app_private_model_dir = app_private_model_dir.into();
        if app_private_model_dir.as_os_str().is_empty() {
            return Err(InferenceError::InvalidModelDirectory);
        }
        bounds.validate()?;
        Ok(Self {
            app_private_model_dir,
            bounds,
        })
    }

    pub fn app_private_model_dir(&self) -> &Path {
        &self.app_private_model_dir
    }

    pub const fn bounds(&self) -> InferenceBounds {
        self.bounds
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct VerifiedModelDescriptor {
    display_name: String,
    file_name: String,
    exact_size: u64,
    sha256: [u8; 32],
}

impl fmt::Debug for VerifiedModelDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedModelDescriptor")
            .field("display_name", &self.display_name)
            .field("file_name", &self.file_name)
            .field("exact_size", &self.exact_size)
            .field("sha256", &hex_sha256(&self.sha256))
            .finish()
    }
}

impl VerifiedModelDescriptor {
    pub fn new(
        display_name: impl Into<String>,
        file_name: impl Into<String>,
        exact_size: u64,
        sha256_hex: &str,
    ) -> Result<Self, InferenceError> {
        let display_name = display_name.into();
        let file_name = file_name.into();
        if display_name.is_empty()
            || display_name.len() > 128
            || display_name.chars().any(char::is_control)
            || exact_size == 0
            || !valid_leaf(&file_name)
        {
            return Err(InferenceError::InvalidModelDescriptor);
        }
        let sha256 = parse_sha256(sha256_hex)?;
        Ok(Self {
            display_name,
            file_name,
            exact_size,
            sha256,
        })
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub const fn exact_size(&self) -> u64 {
        self.exact_size
    }

    pub const fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }

    pub fn sha256_hex(&self) -> String {
        hex_sha256(&self.sha256)
    }
}

#[derive(Clone)]
pub struct PreparedModel {
    descriptor: VerifiedModelDescriptor,
    canonical_path: PathBuf,
    lifecycle_generation: AuthorityGeneration,
    plan_digest: [u8; 32],
    worker_instance: u64,
}

impl fmt::Debug for PreparedModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedModel")
            .field("descriptor", &self.descriptor)
            .field("lifecycle_generation", &self.lifecycle_generation)
            .finish_non_exhaustive()
    }
}

impl PreparedModel {
    pub(crate) fn bind_worker(&mut self, instance: u64) {
        self.worker_instance = instance;
        self.plan_digest = Sha256::new()
            .chain_update(b"argentum.android-worker.v1")
            .chain_update(self.plan_digest)
            .chain_update(instance.to_be_bytes())
            .finalize()
            .into();
    }

    pub(crate) fn belongs_to_worker(&self, instance: u64) -> bool {
        self.worker_instance != 0 && self.worker_instance == instance
    }
    pub fn descriptor(&self) -> &VerifiedModelDescriptor {
        &self.descriptor
    }

    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn lifecycle_generation(&self) -> &AuthorityGeneration {
        &self.lifecycle_generation
    }

    pub const fn plan_digest(&self) -> &[u8; 32] {
        &self.plan_digest
    }

    pub fn plan_digest_hex(&self) -> String {
        hex_sha256(&self.plan_digest)
    }
}

pub struct ActivationApproval {
    plan_digest: [u8; 32],
    lifecycle_generation: AuthorityGeneration,
    unused: bool,
}

impl fmt::Debug for ActivationApproval {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActivationApproval")
            .field("lifecycle_generation", &self.lifecycle_generation)
            .field("unused", &self.unused)
            .finish_non_exhaustive()
    }
}

impl ActivationApproval {
    pub fn bind(prepared: &PreparedModel) -> Self {
        Self {
            plan_digest: prepared.plan_digest,
            lifecycle_generation: prepared.lifecycle_generation.clone(),
            unused: true,
        }
    }

    pub const fn is_unused(&self) -> bool {
        self.unused
    }

    pub(crate) fn consume(
        &mut self,
        prepared: &PreparedModel,
        lifecycle_generation: &AuthorityGeneration,
    ) -> Result<(), InferenceError> {
        if !self.unused
            || &self.lifecycle_generation != lifecycle_generation
            || &prepared.lifecycle_generation != lifecycle_generation
            || self.plan_digest != prepared.plan_digest
        {
            return Err(InferenceError::InvalidActivationApproval);
        }
        self.unused = false;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

impl ChatRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    role: ChatRole,
    content: String,
}

impl ChatMessage {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    pub const fn role(&self) -> ChatRole {
        self.role
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceRequest {
    messages: Vec<ChatMessage>,
    max_generated_tokens: u32,
}

impl InferenceRequest {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        Self {
            messages,
            max_generated_tokens: ANDROID_MAX_GENERATED_TOKENS as u32,
        }
    }

    pub fn with_output_limit(
        messages: Vec<ChatMessage>,
        max_generated_tokens: u32,
    ) -> Result<Self, InferenceError> {
        if max_generated_tokens == 0
            || usize::try_from(max_generated_tokens)
                .map_or(true, |limit| limit > ANDROID_MAX_GENERATED_TOKENS)
        {
            return Err(InferenceError::InvalidRequest);
        }
        Ok(Self {
            messages,
            max_generated_tokens,
        })
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub const fn max_generated_tokens(&self) -> u32 {
        self.max_generated_tokens
    }

    pub(crate) fn validate(&self, bounds: InferenceBounds) -> Result<(), InferenceError> {
        if self.messages.is_empty()
            || self.messages.len() > 128
            || self
                .messages
                .iter()
                .any(|message| message.content.is_empty() || message.content.contains('\0'))
        {
            return Err(InferenceError::InvalidRequest);
        }
        let bytes = self
            .messages
            .iter()
            .try_fold(0usize, |total, message| {
                total.checked_add(message.content.len())
            })
            .ok_or(InferenceError::InvalidRequest)?;
        if bytes > bounds.max_prompt_bytes {
            return Err(InferenceError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Vulkan,
    Cpu,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendReport {
    kind: BackendKind,
    runtime: String,
    context_tokens: u32,
    max_generated_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceInputReport {
    prompt_tokens: u64,
    context_tokens: u32,
    requested_output_tokens: u32,
    template_fingerprint: String,
}

impl InferenceInputReport {
    pub fn new(
        prompt_tokens: u64,
        context_tokens: u32,
        requested_output_tokens: u32,
        template_fingerprint: impl Into<String>,
    ) -> Result<Self, InferenceError> {
        let template_fingerprint = template_fingerprint.into();
        if prompt_tokens == 0
            || context_tokens == 0
            || requested_output_tokens == 0
            || template_fingerprint.len() != 64
            || !template_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(InferenceError::InvalidRequest);
        }
        Ok(Self {
            prompt_tokens,
            context_tokens,
            requested_output_tokens,
            template_fingerprint,
        })
    }

    pub const fn prompt_tokens(&self) -> u64 {
        self.prompt_tokens
    }

    pub const fn context_tokens(&self) -> u32 {
        self.context_tokens
    }

    pub const fn requested_output_tokens(&self) -> u32 {
        self.requested_output_tokens
    }

    pub fn template_fingerprint(&self) -> &str {
        &self.template_fingerprint
    }
}

impl BackendReport {
    pub fn new(kind: BackendKind, runtime: impl Into<String>) -> Result<Self, InferenceError> {
        let runtime = runtime.into();
        if runtime.is_empty() || runtime.len() > 128 || runtime.chars().any(char::is_control) {
            return Err(InferenceError::InvalidModelDescriptor);
        }
        Ok(Self {
            kind,
            runtime,
            context_tokens: ANDROID_CONTEXT_TOKENS,
            max_generated_tokens: ANDROID_MAX_GENERATED_TOKENS,
        })
    }

    pub const fn kind(&self) -> BackendKind {
        self.kind
    }

    pub fn runtime(&self) -> &str {
        &self.runtime
    }

    pub const fn context_tokens(&self) -> u32 {
        self.context_tokens
    }

    pub const fn max_generated_tokens(&self) -> usize {
        self.max_generated_tokens
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamEvent {
    sequence: u64,
    kind: StreamEventKind,
}

impl StreamEvent {
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn kind(&self) -> &StreamEventKind {
        &self.kind
    }

    pub(crate) const fn new(sequence: u64, kind: StreamEventKind) -> Self {
        Self { sequence, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEventKind {
    Token(String),
    Completed,
    Cancelled,
    Failed(String),
}

pub(crate) fn prepare_model(
    config: &InferenceConfig,
    descriptor: VerifiedModelDescriptor,
    lifecycle_generation: AuthorityGeneration,
) -> Result<PreparedModel, InferenceError> {
    if lifecycle_generation.ordinal() == 0 {
        return Err(InferenceError::InvalidGeneration);
    }
    if !valid_leaf(descriptor.file_name()) {
        return Err(InferenceError::InvalidModelLeaf);
    }
    if descriptor.exact_size > config.bounds.max_model_bytes {
        return Err(InferenceError::ModelTooLarge);
    }

    let model_dir = config
        .app_private_model_dir
        .canonicalize()
        .map_err(|_| InferenceError::InvalidModelDirectory)?;
    if !model_dir
        .metadata()
        .map_err(|_| InferenceError::InvalidModelDirectory)?
        .is_dir()
    {
        return Err(InferenceError::InvalidModelDirectory);
    }

    let unresolved = model_dir.join(descriptor.file_name());
    let symlink_metadata = unresolved
        .symlink_metadata()
        .map_err(|_| InferenceError::ModelUnavailable)?;
    if symlink_metadata.file_type().is_symlink() || !symlink_metadata.file_type().is_file() {
        return Err(InferenceError::ModelNotRegularFile);
    }
    let canonical_path = unresolved
        .canonicalize()
        .map_err(|_| InferenceError::ModelUnavailable)?;
    if canonical_path.parent() != Some(model_dir.as_path()) {
        return Err(InferenceError::ModelOutsidePrivateDirectory);
    }
    verify_file(&canonical_path, &descriptor, config.bounds)?;

    let plan_digest = digest_plan(&descriptor, &canonical_path, lifecycle_generation.ordinal());
    Ok(PreparedModel {
        descriptor,
        canonical_path,
        lifecycle_generation,
        plan_digest,
        worker_instance: 0,
    })
}

pub(crate) fn reverify_prepared(
    prepared: &PreparedModel,
    bounds: InferenceBounds,
) -> Result<(), InferenceError> {
    let metadata = prepared
        .canonical_path
        .symlink_metadata()
        .map_err(|_| InferenceError::ModelUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(InferenceError::ModelNotRegularFile);
    }
    verify_file(&prepared.canonical_path, &prepared.descriptor, bounds)
}

fn verify_file(
    path: &Path,
    descriptor: &VerifiedModelDescriptor,
    bounds: InferenceBounds,
) -> Result<(), InferenceError> {
    let file = File::open(path).map_err(|_| InferenceError::ModelUnavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| InferenceError::ModelUnavailable)?;
    if !metadata.is_file() {
        return Err(InferenceError::ModelNotRegularFile);
    }
    if metadata.len() > bounds.max_model_bytes {
        return Err(InferenceError::ModelTooLarge);
    }
    if metadata.len() != descriptor.exact_size {
        return Err(InferenceError::ModelSizeMismatch);
    }

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| InferenceError::ModelUnavailable)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual: [u8; 32] = hasher.finalize().into();
    if actual != descriptor.sha256 {
        return Err(InferenceError::ModelHashMismatch);
    }
    Ok(())
}

fn valid_leaf(value: &str) -> bool {
    if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return false;
    }
    let path = Path::new(value);
    !path.is_absolute()
        && matches!(
            path.components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
}

fn parse_sha256(value: &str) -> Result<[u8; 32], InferenceError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(InferenceError::InvalidModelDescriptor);
    }
    let mut output = [0u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| InferenceError::InvalidModelDescriptor)?;
    }
    Ok(output)
}

fn hex_sha256(value: &[u8; 32]) -> String {
    use fmt::Write as _;
    let mut output = String::with_capacity(64);
    for byte in value {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn digest_plan(
    descriptor: &VerifiedModelDescriptor,
    canonical_path: &Path,
    lifecycle_generation: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"argentum.android.inference.plan.v1\0");
    hash_field(&mut hasher, descriptor.display_name.as_bytes());
    hash_field(&mut hasher, descriptor.file_name.as_bytes());
    hasher.update(descriptor.exact_size.to_le_bytes());
    hasher.update(descriptor.sha256);
    hash_os_str(&mut hasher, canonical_path.as_os_str());
    hasher.update(lifecycle_generation.to_le_bytes());
    hasher.finalize().into()
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

#[cfg(windows)]
fn hash_os_str(hasher: &mut Sha256, value: &OsStr) {
    use std::os::windows::ffi::OsStrExt;
    let units: Vec<u16> = value.encode_wide().collect();
    hasher.update((units.len() as u64).to_le_bytes());
    for unit in units {
        hasher.update(unit.to_le_bytes());
    }
}

#[cfg(unix)]
fn hash_os_str(hasher: &mut Sha256, value: &OsStr) {
    use std::os::unix::ffi::OsStrExt;
    hash_field(hasher, value.as_bytes());
}

#[cfg(not(any(unix, windows)))]
fn hash_os_str(hasher: &mut Sha256, value: &OsStr) {
    hash_field(hasher, value.to_string_lossy().as_bytes());
}
