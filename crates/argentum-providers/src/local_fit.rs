// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Whether a GGUF model fits this computer, judged with the same numbers the
//! local runtime launches with.
//!
//! The launcher starts llama.cpp with `--fit on`, which keeps a free margin on
//! each GPU (`--fit-target`, 2,048 MiB when a vision projector is loaded and
//! llama.cpp's default of 1,024 MiB otherwise) and puts whatever does not fit
//! there in system memory. It runs llama-server in a Windows Job Object whose
//! memory limit is [`ManagedProcessLimits::for_local_model`], and it refuses a
//! model larger than its size limit. This module applies those same rules to a
//! file size before anything is downloaded.
//!
//! The KV cache is not in the file. Its size depends on the architecture, which
//! the Hub does not list, so it is estimated from the model's size and the
//! launcher's context length. The estimate is labeled as one, and llama.cpp
//! still measures the real free memory at launch.

use argentum_execution::ManagedProcessLimits;

use crate::local_runtime::{
    LOCAL_RUNTIME_CONTEXT, LOCAL_RUNTIME_MAX_MODEL_BYTES, LOCAL_RUNTIME_MMPROJ_FIT_TARGET_MIB,
};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
/// llama.cpp's `--fit-target` default, used when the launcher does not set one.
pub const LLAMA_CPP_DEFAULT_FIT_TARGET_MIB: u32 = 1_024;
/// System memory left for Windows and other programs.
pub const SYSTEM_MEMORY_RESERVE_BYTES: u64 = 4 * GIB;
/// KV cache per context token for a model of about 4.5 GB, at f16, which is
/// what an 8B model with grouped-query attention needs.
const KV_BYTES_PER_TOKEN_AT_REFERENCE: u64 = 128 * 1024;
const KV_REFERENCE_MODEL_BYTES: u64 = 4_500_000_000;

/// The memory this computer has, read locally.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryFacts {
    pub system_memory_bytes: u64,
    /// Dedicated memory of the GPU llama.cpp would use, the largest one.
    pub gpu_memory_bytes: Option<u64>,
}

/// Where a model would run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalModelFit {
    /// Weights and KV cache fit in GPU memory with the launcher's margin.
    FullGpu,
    /// Part of the model stays in system memory. It runs, more slowly.
    Offload,
    /// No GPU was found. It runs on the CPU from system memory.
    CpuOnly,
    /// More than GPU and system memory together, or over the launcher's limit.
    TooLarge,
}

impl LocalModelFit {
    pub fn label(self) -> &'static str {
        match self {
            Self::FullGpu => "Fits fully on GPU",
            Self::Offload => "Fits with offload to system memory",
            Self::CpuOnly => "Runs on the CPU",
            Self::TooLarge => "Will not fit",
        }
    }
}

/// The verdict with the numbers behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalModelFitEstimate {
    pub fit: LocalModelFit,
    /// Weights plus the estimated KV cache.
    pub needed_bytes: u64,
    pub kv_cache_bytes: u64,
    /// GPU memory llama.cpp may fill, after the launcher's margin.
    pub gpu_usable_bytes: u64,
    /// The Job Object memory limit the launcher would set.
    pub process_limit_bytes: u64,
}

/// The `--fit-target` margin the launcher uses, in MiB.
pub fn launcher_fit_target_mib(has_projector: bool) -> u32 {
    if has_projector {
        LOCAL_RUNTIME_MMPROJ_FIT_TARGET_MIB
    } else {
        LLAMA_CPP_DEFAULT_FIT_TARGET_MIB
    }
}

/// The launcher's default context length.
pub fn launcher_context_tokens() -> u32 {
    LOCAL_RUNTIME_CONTEXT
}

/// An estimate of the KV cache for `context_tokens`, scaled by model size and
/// kept between a quarter and four times the reference.
pub fn estimated_kv_cache_bytes(model_bytes: u64, context_tokens: u32) -> u64 {
    let scale_milli =
        (model_bytes.saturating_mul(1000) / KV_REFERENCE_MODEL_BYTES).clamp(250, 4000);
    u64::from(context_tokens)
        .saturating_mul(KV_BYTES_PER_TOKEN_AT_REFERENCE)
        .saturating_mul(scale_milli)
        / 1000
}

/// Whether a model of `model_bytes` (all parts, plus any projector) fits.
pub fn local_model_fit(
    model_bytes: u64,
    has_projector: bool,
    context_tokens: u32,
    memory: &MemoryFacts,
) -> LocalModelFitEstimate {
    let kv_cache_bytes = estimated_kv_cache_bytes(model_bytes, context_tokens);
    let needed_bytes = model_bytes.saturating_add(kv_cache_bytes);
    let margin = u64::from(launcher_fit_target_mib(has_projector)) * MIB;
    let gpu_usable_bytes = memory
        .gpu_memory_bytes
        .map_or(0, |gpu| gpu.saturating_sub(margin));
    let process_limit = ManagedProcessLimits::for_local_model(model_bytes)
        .map(|limits| limits.job_memory_bytes())
        .ok();
    let process_limit_bytes = process_limit.unwrap_or(0);
    let system_usable = memory
        .system_memory_bytes
        .saturating_sub(SYSTEM_MEMORY_RESERVE_BYTES);
    let in_system_memory = needed_bytes.saturating_sub(gpu_usable_bytes);
    let fit = if model_bytes > LOCAL_RUNTIME_MAX_MODEL_BYTES || process_limit.is_none() {
        LocalModelFit::TooLarge
    } else if memory.gpu_memory_bytes.is_some() && needed_bytes <= gpu_usable_bytes {
        LocalModelFit::FullGpu
    } else if in_system_memory > system_usable || in_system_memory > process_limit_bytes {
        LocalModelFit::TooLarge
    } else if memory.gpu_memory_bytes.is_some() {
        LocalModelFit::Offload
    } else {
        LocalModelFit::CpuOnly
    };
    LocalModelFitEstimate {
        fit,
        needed_bytes,
        kv_cache_bytes,
        gpu_usable_bytes,
        process_limit_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn pc(ram_gib: u64, vram_gib: Option<u64>) -> MemoryFacts {
        MemoryFacts {
            system_memory_bytes: ram_gib * GIB,
            gpu_memory_bytes: vram_gib.map(|gib| gib * GIB),
        }
    }

    #[test]
    fn the_margins_are_the_launchers() {
        assert_eq!(
            launcher_fit_target_mib(true),
            LOCAL_RUNTIME_MMPROJ_FIT_TARGET_MIB
        );
        assert_eq!(launcher_fit_target_mib(false), 1_024);
        assert_eq!(launcher_context_tokens(), crate::SIDECAR_CONTEXT_TOKENS);
        let estimate = local_model_fit(2 * GB, false, 8192, &pc(32, Some(8)));
        assert_eq!(
            estimate.process_limit_bytes,
            ManagedProcessLimits::for_local_model(2 * GB)
                .expect("limits")
                .job_memory_bytes()
        );
        assert_eq!(estimate.gpu_usable_bytes, 7 * GIB);
    }

    #[test]
    fn small_models_fit_the_gpu_and_large_ones_offload() {
        // Qwen3.5 4B Q4_K_M on an 8 GB GPU.
        let small = local_model_fit(2_740_937_888, false, 8192, &pc(32, Some(8)));
        assert_eq!(small.fit, LocalModelFit::FullGpu, "{small:?}");
        // Inkling with its projector: 17.2 GB on an 8 GB GPU and 32 GB of RAM,
        // which the add-on measured running with 25 of 49 layers on the GPU.
        let inkling = local_model_fit(17_208_713_408, true, 16384, &pc(32, Some(8)));
        assert_eq!(inkling.fit, LocalModelFit::Offload, "{inkling:?}");
        // The same model on 16 GB of RAM leaves too little for the rest.
        let tight = local_model_fit(17_208_713_408, true, 16384, &pc(16, Some(8)));
        assert_eq!(tight.fit, LocalModelFit::TooLarge, "{tight:?}");
    }

    #[test]
    fn no_gpu_means_cpu_only_or_too_large() {
        assert_eq!(
            local_model_fit(4 * GB, false, 8192, &pc(16, None)).fit,
            LocalModelFit::CpuOnly
        );
        assert_eq!(
            local_model_fit(40 * GB, false, 8192, &pc(16, None)).fit,
            LocalModelFit::TooLarge
        );
        assert_eq!(
            local_model_fit(
                LOCAL_RUNTIME_MAX_MODEL_BYTES + 1,
                false,
                8192,
                &pc(1024, Some(512))
            )
            .fit,
            LocalModelFit::TooLarge,
            "over the launcher's size limit"
        );
    }

    #[test]
    fn the_kv_estimate_grows_with_context_and_model_size() {
        let reference = estimated_kv_cache_bytes(KV_REFERENCE_MODEL_BYTES, 8192);
        assert_eq!(reference, GIB);
        assert_eq!(
            estimated_kv_cache_bytes(KV_REFERENCE_MODEL_BYTES, 16384),
            2 * GIB
        );
        assert_eq!(estimated_kv_cache_bytes(1, 8192), GIB / 4, "clamped below");
        assert_eq!(
            estimated_kv_cache_bytes(u64::MAX / 2000, 8192),
            4 * GIB,
            "clamped above"
        );
    }
}
