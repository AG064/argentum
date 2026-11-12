#![forbid(unsafe_code)]
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

mod module;

/// Whether this build can run a model on an Android device.
pub const ENGINE_INCLUDED: bool = cfg!(all(target_os = "android", feature = "llama-engine"));
mod types;

#[cfg(all(target_os = "android", feature = "llama-engine"))]
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
