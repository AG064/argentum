use std::num::NonZeroU32;

use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaChatMessage, LlamaModel},
    sampling::LlamaSampler,
    LlamaBackendDeviceType,
};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::{
    BackendKind, BackendReport, EngineFailure, EngineStreamOutcome, InferenceEngine,
    InferenceInputReport, InferenceRequest, PreparedModel, ANDROID_CONTEXT_TOKENS,
};

pub(crate) struct AndroidLlamaCppEngine {
    backend: LlamaBackend,
    model: Option<LlamaModel>,
}

impl AndroidLlamaCppEngine {
    pub(crate) fn create() -> Result<Box<dyn InferenceEngine>, EngineFailure> {
        let mut backend = LlamaBackend::init().map_err(engine_failure)?;
        backend.void_logs();
        Ok(Box::new(Self {
            backend,
            model: None,
        }))
    }

    fn load_with_preference(
        &self,
        prepared: &PreparedModel,
    ) -> Result<(LlamaModel, BackendKind), EngineFailure> {
        let vulkan_available = self.backend.supports_gpu_offload()
            && llama_cpp_2::list_llama_ggml_backend_devices()
                .iter()
                .any(|device| {
                    device.backend.eq_ignore_ascii_case("vulkan")
                        && matches!(
                            device.device_type,
                            LlamaBackendDeviceType::Gpu | LlamaBackendDeviceType::IntegratedGpu
                        )
                });

        if vulkan_available {
            let parameters = LlamaModelParams::default().with_n_gpu_layers(u32::MAX);
            if let Ok(model) =
                LlamaModel::load_from_file(&self.backend, prepared.canonical_path(), &parameters)
            {
                return Ok((model, BackendKind::Vulkan));
            }
        }

        let model = LlamaModel::load_from_file(
            &self.backend,
            prepared.canonical_path(),
            &LlamaModelParams::default(),
        )
        .map_err(engine_failure)?;
        Ok((model, BackendKind::Cpu))
    }

    fn prepare_input(
        &self,
        request: &InferenceRequest,
    ) -> Result<(Vec<llama_cpp_2::token::LlamaToken>, String), EngineFailure> {
        let model = self
            .model
            .as_ref()
            .ok_or_else(|| EngineFailure::new("no model is loaded"))?;
        let template = model.chat_template(None).map_err(engine_failure)?;
        let template_fingerprint = format!("{:x}", Sha256::digest(template.as_c_str().to_bytes()));
        let messages = request
            .messages()
            .iter()
            .map(|message| {
                LlamaChatMessage::new(message.role().as_str().into(), message.content().into())
                    .map_err(engine_failure)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let prompt = model
            .apply_chat_template(&template, &messages, true)
            .map_err(engine_failure)?;
        let prompt_tokens = model
            .str_to_token(&prompt, AddBos::Never)
            .map_err(engine_failure)?;
        if prompt_tokens.is_empty() {
            return Err(EngineFailure::new("final native prompt is empty"));
        }
        Ok((prompt_tokens, template_fingerprint))
    }
}

impl InferenceEngine for AndroidLlamaCppEngine {
    fn load(&mut self, model: &PreparedModel) -> Result<BackendReport, EngineFailure> {
        let (loaded, kind) = self.load_with_preference(model)?;
        self.model = Some(loaded);
        BackendReport::new(kind, "llama-cpp-2 0.1.156")
            .map_err(|error| EngineFailure::new(error.to_string()))
    }

    fn stream(
        &mut self,
        request: &InferenceRequest,
        cancellation: &CancellationToken,
        emit: &mut dyn FnMut(&str) -> Result<(), EngineFailure>,
    ) -> Result<EngineStreamOutcome, EngineFailure> {
        let model = self
            .model
            .as_ref()
            .ok_or_else(|| EngineFailure::new("no model is loaded"))?;
        let (prompt_tokens, _) = self.prepare_input(request)?;
        let maximum_prompt_tokens = usize::try_from(ANDROID_CONTEXT_TOKENS)
            .map_err(engine_failure)?
            .saturating_sub(
                usize::try_from(request.max_generated_tokens()).map_err(engine_failure)?,
            );
        if prompt_tokens.len() > maximum_prompt_tokens {
            return Err(EngineFailure::new(
                "prompt and requested output do not fit the configured context",
            ));
        }
        if cancellation.is_cancelled() {
            return Ok(EngineStreamOutcome::Cancelled);
        }

        let thread_count = std::thread::available_parallelism()
            .map(|count| count.get().min(8))
            .unwrap_or(1);
        let thread_count = i32::try_from(thread_count).map_err(engine_failure)?;
        let context_parameters = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(ANDROID_CONTEXT_TOKENS))
            .with_n_batch(ANDROID_CONTEXT_TOKENS)
            .with_n_threads(thread_count)
            .with_n_threads_batch(thread_count);
        let mut context = model
            .new_context(&self.backend, context_parameters)
            .map_err(engine_failure)?;
        let mut batch = LlamaBatch::new(prompt_tokens.len().max(1), 1);
        batch
            .add_sequence(&prompt_tokens, 0, false)
            .map_err(engine_failure)?;
        context.decode(&mut batch).map_err(engine_failure)?;

        let mut sampler = LlamaSampler::greedy();
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut position = i32::try_from(prompt_tokens.len()).map_err(engine_failure)?;
        for _ in 0..request.max_generated_tokens() {
            if cancellation.is_cancelled() {
                return Ok(EngineStreamOutcome::Cancelled);
            }
            let token = sampler.sample(&context, -1);
            sampler.accept(token);
            if model.is_eog_token(token) {
                return Ok(EngineStreamOutcome::Completed);
            }
            let fragment = model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(engine_failure)?;
            if !fragment.is_empty() {
                emit(&fragment)?;
            }
            if cancellation.is_cancelled() {
                return Ok(EngineStreamOutcome::Cancelled);
            }
            batch.clear();
            batch
                .add(token, position, &[0], true)
                .map_err(engine_failure)?;
            context.decode(&mut batch).map_err(engine_failure)?;
            position = position
                .checked_add(1)
                .ok_or_else(|| EngineFailure::new("token position exhausted"))?;
        }
        Ok(EngineStreamOutcome::Completed)
    }

    fn account(
        &mut self,
        request: &InferenceRequest,
    ) -> Result<InferenceInputReport, EngineFailure> {
        let (tokens, template_fingerprint) = self.prepare_input(request)?;
        InferenceInputReport::new(
            tokens.len() as u64,
            ANDROID_CONTEXT_TOKENS,
            request.max_generated_tokens(),
            template_fingerprint,
        )
        .map_err(|error| EngineFailure::new(error.to_string()))
    }

    fn unload(&mut self) -> Result<(), EngineFailure> {
        self.model = None;
        Ok(())
    }
}

fn engine_failure(error: impl std::fmt::Display) -> EngineFailure {
    EngineFailure::new(error.to_string())
}
