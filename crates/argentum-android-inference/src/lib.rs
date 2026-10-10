#![forbid(unsafe_code)]

mod module;
mod types;

#[cfg(target_os = "android")]
mod android_engine;

pub use module::{
    AndroidInferenceModule, EngineFailure, EngineStreamOutcome, InferenceEngine,
    InferenceEngineFactory, StreamHandle,
};
pub use types::{
    ActivationApproval, BackendKind, BackendReport, ChatMessage, ChatRole, InferenceBounds,
    InferenceConfig, InferenceError, InferenceInputReport, InferenceRequest, PreparedModel,
    StreamEvent, StreamEventKind, VerifiedModelDescriptor, ANDROID_CONTEXT_TOKENS,
    ANDROID_MAX_GENERATED_TOKENS,
};
