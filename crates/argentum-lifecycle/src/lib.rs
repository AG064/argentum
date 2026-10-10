use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    panic::AssertUnwindSafe,
    pin::Pin,
};

use async_trait::async_trait;
use futures_util::FutureExt;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MAX_MODULES: usize = 128;
const MAX_DEPENDENCIES: usize = 16;
const MAX_EVENTS: usize = 256;
const MAX_FAILURE_BYTES: usize = 512;

pub type MountFuture<'a> = Pin<Box<dyn Future<Output = Result<(), MountFailure>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationBinding {
    kernel_id: Uuid,
    generation: u64,
}

impl GenerationBinding {
    pub const fn kernel_id(&self) -> Uuid {
        self.kernel_id
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectFailure {
    message: String,
}

impl EffectFailure {
    pub fn new(message: impl AsRef<str>) -> Self {
        Self {
            message: bounded_failure_message(message.as_ref()),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for EffectFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EffectFailure {}

#[async_trait]
pub trait EffectDisposer: Send {
    async fn dispose(&mut self, cancellation: &CancellationToken) -> Result<(), EffectFailure>;
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MountFailure {
    #[error("module setup was cancelled")]
    Cancelled,
    #[error("effect label must use 1 to 128 printable ASCII bytes")]
    InvalidEffectLabel,
    #[error("module effects are limited to 64 entries")]
    EffectLimit,
    #[error("module setup failed: {0}")]
    Module(EffectFailure),
    #[error("effect '{label}' could not be applied: {failure}")]
    EffectApply {
        label: String,
        failure: EffectFailure,
    },
    #[error("module setup panicked")]
    Panicked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    Module,
    Disposer,
    Effect,
}

impl std::fmt::Display for HandleKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Module => "module",
            Self::Disposer => "disposer",
            Self::Effect => "effect",
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CleanupFailure {
    #[error("cleanup was cancelled")]
    Cancelled,
    #[error("effect '{label}' cleanup failed: {failure}")]
    Effect {
        label: String,
        failure: EffectFailure,
    },
}

impl MountFailure {
    pub fn module(message: impl AsRef<str>) -> Self {
        Self::Module(EffectFailure::new(message))
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    #[error("generation binding belongs to another lifecycle kernel")]
    ForeignBinding,
    #[error("generation binding is stale: expected {expected}, received {received}")]
    StaleBinding { expected: u64, received: u64 },
    #[error("lifecycle generation is exhausted")]
    GenerationExhausted,
    #[error("module ID must use 1 to 64 lowercase ASCII letters, digits, dots, or hyphens")]
    InvalidModuleId,
    #[error("module dependencies are limited to 16 entries")]
    DependencyLimit,
    #[error("module dependency '{0}' is listed more than once")]
    DuplicateDependency(String),
    #[error("module '{0}' cannot depend on itself")]
    SelfDependency(String),
    #[error("lifecycle module limit reached")]
    ModuleLimit,
    #[error("lifecycle module '{0}' already exists")]
    DuplicateModule(String),
    #[error("module '{module_id}' requires mounted dependency '{dependency}'")]
    DependencyNotMounted {
        module_id: String,
        dependency: String,
    },
    #[error("mount for module '{module_id}' was cancelled")]
    MountCancelled { module_id: String },
    #[error("mount for module '{module_id}' failed: {failure}")]
    MountFailed {
        module_id: String,
        failure: MountFailure,
    },
    #[error("rollback for module '{module_id}' is incomplete after {failure}: {cleanup}")]
    RollbackPending {
        module_id: String,
        failure: MountFailure,
        cleanup: EffectFailure,
        recovery: Box<Recovery>,
    },
    #[error("lifecycle {0} handle belongs to another kernel")]
    ForeignHandle(HandleKind),
    #[error("lifecycle {0} handle is stale")]
    StaleHandle(HandleKind),
    #[error("module '{module_id}' is still required by '{dependent}'")]
    DependentMounted {
        module_id: String,
        dependent: String,
    },
    #[error("lifecycle dependency graph contains a cycle reachable from '{module_id}'")]
    DependencyCycle { module_id: String },
    #[error("teardown for module '{module_id}' was cancelled")]
    TeardownCancelled { module_id: String },
    #[error("teardown for module '{module_id}' is incomplete: {failure}")]
    TeardownPending {
        module_id: String,
        failure: CleanupFailure,
        recovery: Box<Recovery>,
    },
    #[error("module '{module_id}' has cleanup pending in state {state:?}")]
    ModuleCleanupPending {
        module_id: String,
        state: ModuleState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSpec {
    id: String,
    dependencies: Vec<String>,
}

impl ModuleSpec {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            dependencies: Vec::new(),
        }
    }

    pub fn with_dependencies<I, S>(id: impl Into<String>, dependencies: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            id: id.into(),
            dependencies: dependencies.into_iter().map(Into::into).collect(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleState {
    Mounted,
    RollbackPending,
    TeardownPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEventKind {
    Mounted,
    RollbackComplete,
    RollbackPending,
    TeardownComplete,
    TeardownPending,
    EffectDisposed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleEvent {
    sequence: u128,
    generation: u64,
    module_id: String,
    kind: LifecycleEventKind,
}

impl LifecycleEvent {
    pub const fn sequence(&self) -> u128 {
        self.sequence
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub const fn kind(&self) -> LifecycleEventKind {
        self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSnapshot {
    id: String,
    mount_generation: u64,
    state: ModuleState,
    dependencies: Vec<String>,
    effects: Vec<EffectSnapshot>,
}

impl ModuleSnapshot {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub const fn mount_generation(&self) -> u64 {
        self.mount_generation
    }

    pub const fn state(&self) -> ModuleState {
        self.state
    }

    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    pub fn effects(&self) -> &[EffectSnapshot] {
        &self.effects
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSnapshot {
    sequence: u64,
    label: String,
}

impl EffectSnapshot {
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleSnapshot {
    kernel_id: Uuid,
    generation: u64,
    modules: Vec<ModuleSnapshot>,
    events: Vec<LifecycleEvent>,
}

impl LifecycleSnapshot {
    pub const fn kernel_id(&self) -> Uuid {
        self.kernel_id
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn modules(&self) -> &[ModuleSnapshot] {
        &self.modules
    }

    pub fn events(&self) -> &[LifecycleEvent] {
        &self.events
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleHandle {
    kernel_id: Uuid,
    module_id: String,
    mount_generation: u64,
}

impl ModuleHandle {
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub const fn mount_generation(&self) -> u64 {
        self.mount_generation
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisposerHandle {
    kernel_id: Uuid,
    module_id: String,
    mount_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectHandle {
    kernel_id: Uuid,
    module_id: String,
    mount_generation: u64,
    sequence: u64,
}

impl EffectHandle {
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub const fn mount_generation(&self) -> u64 {
        self.mount_generation
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    binding: GenerationBinding,
    module: ModuleHandle,
    disposer: DisposerHandle,
}

impl Recovery {
    pub const fn binding(&self) -> GenerationBinding {
        self.binding
    }

    pub fn module(&self) -> &ModuleHandle {
        &self.module
    }

    pub fn disposer(&self) -> &DisposerHandle {
        &self.disposer
    }
}

impl DisposerHandle {
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub const fn mount_generation(&self) -> u64 {
        self.mount_generation
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedModule {
    binding: GenerationBinding,
    module: ModuleHandle,
    disposer: DisposerHandle,
}

impl MountedModule {
    pub const fn binding(&self) -> GenerationBinding {
        self.binding
    }

    pub fn module(&self) -> &ModuleHandle {
        &self.module
    }

    pub fn disposer(&self) -> &DisposerHandle {
        &self.disposer
    }
}

pub struct MountContext {
    kernel_id: Uuid,
    module_id: String,
    mount_generation: u64,
    cancellation: CancellationToken,
    effects: Vec<EffectRecord>,
    next_effect_sequence: u64,
}

impl MountContext {
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub const fn mount_generation(&self) -> u64 {
        self.mount_generation
    }

    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub async fn apply_effect<A, ApplyFuture, D>(
        &mut self,
        label: impl Into<String>,
        apply: A,
    ) -> Result<EffectHandle, MountFailure>
    where
        A: FnOnce() -> ApplyFuture + Send,
        ApplyFuture: Future<Output = Result<D, EffectFailure>> + Send,
        D: EffectDisposer + 'static,
    {
        if self.cancellation.is_cancelled() {
            return Err(MountFailure::Cancelled);
        }
        let label = label.into();
        if !valid_effect_label(&label) {
            return Err(MountFailure::InvalidEffectLabel);
        }
        if self.effects.len() >= 64 {
            return Err(MountFailure::EffectLimit);
        }

        let apply_future = std::panic::catch_unwind(AssertUnwindSafe(apply)).map_err(|_| {
            MountFailure::EffectApply {
                label: label.clone(),
                failure: EffectFailure::new("effect apply panicked"),
            }
        })?;
        let disposer = AssertUnwindSafe(apply_future)
            .catch_unwind()
            .await
            .map_err(|_| MountFailure::EffectApply {
                label: label.clone(),
                failure: EffectFailure::new("effect apply panicked"),
            })?
            .map_err(|failure| MountFailure::EffectApply {
                label: label.clone(),
                failure,
            })?;
        let sequence = self.next_effect_sequence;
        self.next_effect_sequence += 1;
        self.effects.push(EffectRecord {
            sequence,
            label,
            disposer: Box::new(disposer),
        });
        let handle = EffectHandle {
            kernel_id: self.kernel_id,
            module_id: self.module_id.clone(),
            mount_generation: self.mount_generation,
            sequence,
        };
        if self.cancellation.is_cancelled() {
            return Err(MountFailure::Cancelled);
        }
        Ok(handle)
    }
}

struct EffectRecord {
    sequence: u64,
    label: String,
    disposer: Box<dyn EffectDisposer>,
}

struct ModuleRecord {
    mount_generation: u64,
    state: ModuleState,
    dependencies: Vec<String>,
    effects: Vec<EffectRecord>,
}

pub struct LifecycleKernel {
    kernel_id: Uuid,
    generation: u64,
    modules: BTreeMap<String, ModuleRecord>,
    events: VecDeque<LifecycleEvent>,
    next_event_sequence: u128,
}

impl Default for LifecycleKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleKernel {
    pub fn new() -> Self {
        Self {
            kernel_id: Uuid::new_v4(),
            generation: 0,
            modules: BTreeMap::new(),
            events: VecDeque::new(),
            next_event_sequence: 1,
        }
    }

    pub const fn binding(&self) -> GenerationBinding {
        GenerationBinding {
            kernel_id: self.kernel_id,
            generation: self.generation,
        }
    }

    pub fn snapshot(&self) -> LifecycleSnapshot {
        LifecycleSnapshot {
            kernel_id: self.kernel_id,
            generation: self.generation,
            modules: self
                .modules
                .iter()
                .map(|(id, record)| module_snapshot(id, record))
                .collect(),
            events: self.events.iter().cloned().collect(),
        }
    }

    pub fn validate_mount(
        &self,
        binding: GenerationBinding,
        spec: &ModuleSpec,
    ) -> Result<(), LifecycleError> {
        self.validate_binding(binding)?;
        validate_module_spec(spec)?;
        if self.modules.contains_key(&spec.id) {
            return Err(LifecycleError::DuplicateModule(spec.id.clone()));
        }
        if self.modules.len() >= MAX_MODULES {
            return Err(LifecycleError::ModuleLimit);
        }
        for dependency in &spec.dependencies {
            let Some(record) = self.modules.get(dependency) else {
                return Err(LifecycleError::DependencyNotMounted {
                    module_id: spec.id.clone(),
                    dependency: dependency.clone(),
                });
            };
            if record.state != ModuleState::Mounted {
                return Err(LifecycleError::DependencyNotMounted {
                    module_id: spec.id.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
        Ok(())
    }

    pub async fn mount<F>(
        &mut self,
        binding: GenerationBinding,
        spec: ModuleSpec,
        cancellation: &CancellationToken,
        configure: F,
    ) -> Result<MountedModule, LifecycleError>
    where
        F: for<'a> FnOnce(&'a mut MountContext) -> MountFuture<'a>,
    {
        self.mount_with_followup(binding, spec, cancellation, configure, |_| {
            Box::pin(async { Ok(()) })
        })
        .await
    }

    pub async fn mount_with_followup<F, G>(
        &mut self,
        binding: GenerationBinding,
        spec: ModuleSpec,
        cancellation: &CancellationToken,
        configure: F,
        followup: G,
    ) -> Result<MountedModule, LifecycleError>
    where
        F: for<'a> FnOnce(&'a mut MountContext) -> MountFuture<'a>,
        G: for<'a> FnOnce(&'a mut MountContext) -> MountFuture<'a>,
    {
        self.validate_mount(binding, &spec)?;
        if cancellation.is_cancelled() {
            return Err(LifecycleError::MountCancelled { module_id: spec.id });
        }

        let mount_generation = self
            .generation
            .checked_add(1)
            .ok_or(LifecycleError::GenerationExhausted)?;
        let mut context = MountContext {
            kernel_id: self.kernel_id,
            module_id: spec.id.clone(),
            mount_generation,
            cancellation: cancellation.clone(),
            effects: Vec::new(),
            next_effect_sequence: 1,
        };
        let setup_result = {
            let setup_future =
                std::panic::catch_unwind(AssertUnwindSafe(|| configure(&mut context)));
            match setup_future {
                Ok(future) => AssertUnwindSafe(future)
                    .catch_unwind()
                    .await
                    .unwrap_or(Err(MountFailure::Panicked)),
                Err(_) => Err(MountFailure::Panicked),
            }
        };
        let setup_result = if setup_result.is_ok() && !cancellation.is_cancelled() {
            let followup_future =
                std::panic::catch_unwind(AssertUnwindSafe(|| followup(&mut context)));
            match followup_future {
                Ok(future) => AssertUnwindSafe(future)
                    .catch_unwind()
                    .await
                    .unwrap_or(Err(MountFailure::Panicked)),
                Err(_) => Err(MountFailure::Panicked),
            }
        } else {
            setup_result
        };
        let setup_result = if cancellation.is_cancelled() {
            Err(MountFailure::Cancelled)
        } else {
            setup_result
        };
        if let Err(failure) = setup_result {
            let (remaining, cleanup) = rollback_effects(context.effects).await;
            if let Some(cleanup) = cleanup {
                self.generation = mount_generation;
                self.modules.insert(
                    spec.id.clone(),
                    ModuleRecord {
                        mount_generation,
                        state: ModuleState::RollbackPending,
                        dependencies: spec.dependencies,
                        effects: remaining,
                    },
                );
                self.push_event(
                    self.generation,
                    spec.id.clone(),
                    LifecycleEventKind::RollbackPending,
                );
                let recovery = self.recovery(&spec.id, mount_generation);
                return Err(LifecycleError::RollbackPending {
                    module_id: spec.id,
                    failure,
                    cleanup,
                    recovery: Box::new(recovery),
                });
            }
            self.push_event(
                self.generation,
                spec.id.clone(),
                LifecycleEventKind::RollbackComplete,
            );
            return Err(match failure {
                MountFailure::Cancelled => LifecycleError::MountCancelled { module_id: spec.id },
                failure => LifecycleError::MountFailed {
                    module_id: spec.id,
                    failure,
                },
            });
        }

        self.generation = mount_generation;
        self.modules.insert(
            spec.id.clone(),
            ModuleRecord {
                mount_generation,
                state: ModuleState::Mounted,
                dependencies: spec.dependencies,
                effects: context.effects,
            },
        );
        self.push_event(
            self.generation,
            spec.id.clone(),
            LifecycleEventKind::Mounted,
        );
        let binding = self.binding();
        Ok(MountedModule {
            binding,
            module: ModuleHandle {
                kernel_id: self.kernel_id,
                module_id: spec.id.clone(),
                mount_generation,
            },
            disposer: DisposerHandle {
                kernel_id: self.kernel_id,
                module_id: spec.id,
                mount_generation,
            },
        })
    }

    pub fn inspect_module(&self, handle: &ModuleHandle) -> Result<ModuleSnapshot, LifecycleError> {
        self.validate_module_handle(handle)?;
        let record = self
            .modules
            .get(&handle.module_id)
            .ok_or(LifecycleError::StaleHandle(HandleKind::Module))?;
        Ok(module_snapshot(&handle.module_id, record))
    }

    pub async fn unmount(
        &mut self,
        binding: GenerationBinding,
        module: &ModuleHandle,
        disposer: &DisposerHandle,
        cancellation: &CancellationToken,
    ) -> Result<GenerationBinding, LifecycleError> {
        self.validate_binding(binding)?;
        self.validate_module_handle(module)?;
        self.validate_disposer_handle(disposer, module)?;
        if let Some((dependent, _)) = self.modules.iter().find(|(candidate_id, record)| {
            candidate_id.as_str() != module.module_id
                && record
                    .dependencies
                    .iter()
                    .any(|dependency| dependency == &module.module_id)
        }) {
            return Err(LifecycleError::DependentMounted {
                module_id: module.module_id.clone(),
                dependent: dependent.clone(),
            });
        }
        if cancellation.is_cancelled() {
            return Err(LifecycleError::TeardownCancelled {
                module_id: module.module_id.clone(),
            });
        }
        let next_generation = self
            .generation
            .checked_add(1)
            .ok_or(LifecycleError::GenerationExhausted)?;
        let mut record = self
            .modules
            .remove(&module.module_id)
            .ok_or(LifecycleError::StaleHandle(HandleKind::Module))?;
        let original_state = record.state;
        let mut attempted = false;

        while !record.effects.is_empty() {
            if cancellation.is_cancelled() {
                if !attempted {
                    self.modules.insert(module.module_id.clone(), record);
                    return Err(LifecycleError::TeardownCancelled {
                        module_id: module.module_id.clone(),
                    });
                }
                return Err(self.retain_pending_cleanup(
                    module,
                    record,
                    original_state,
                    next_generation,
                    CleanupFailure::Cancelled,
                ));
            }

            attempted = true;
            let effect_index = record.effects.len() - 1;
            let effect = &mut record.effects[effect_index];
            let label = effect.label.clone();
            match dispose_record(effect, cancellation).await {
                Ok(()) => {
                    record.effects.pop();
                    if cancellation.is_cancelled() && !record.effects.is_empty() {
                        return Err(self.retain_pending_cleanup(
                            module,
                            record,
                            original_state,
                            next_generation,
                            CleanupFailure::Cancelled,
                        ));
                    }
                }
                Err(failure) => {
                    return Err(self.retain_pending_cleanup(
                        module,
                        record,
                        original_state,
                        next_generation,
                        CleanupFailure::Effect { label, failure },
                    ));
                }
            }
        }

        self.generation = next_generation;
        let event = if original_state == ModuleState::RollbackPending {
            LifecycleEventKind::RollbackComplete
        } else {
            LifecycleEventKind::TeardownComplete
        };
        self.push_event(self.generation, module.module_id.clone(), event);
        Ok(self.binding())
    }

    pub async fn retire_tree(
        &mut self,
        binding: GenerationBinding,
        root: &ModuleHandle,
        cancellation: &CancellationToken,
    ) -> Result<GenerationBinding, LifecycleError> {
        self.validate_binding(binding)?;
        self.validate_module_handle(root)?;
        if cancellation.is_cancelled() {
            return Err(LifecycleError::TeardownCancelled {
                module_id: root.module_id.clone(),
            });
        }

        let plan = self.plan_retirement(&root.module_id)?;
        for module_id in plan {
            let mount_generation = self
                .modules
                .get(&module_id)
                .ok_or(LifecycleError::StaleHandle(HandleKind::Module))?
                .mount_generation;
            let module = ModuleHandle {
                kernel_id: self.kernel_id,
                module_id: module_id.clone(),
                mount_generation,
            };
            let disposer = DisposerHandle {
                kernel_id: self.kernel_id,
                module_id,
                mount_generation,
            };
            self.unmount(self.binding(), &module, &disposer, cancellation)
                .await?;
        }
        Ok(self.binding())
    }

    pub async fn dispose_effect(
        &mut self,
        binding: GenerationBinding,
        module: &ModuleHandle,
        effect: &EffectHandle,
        cancellation: &CancellationToken,
    ) -> Result<GenerationBinding, LifecycleError> {
        self.validate_binding(binding)?;
        self.validate_module_handle(module)?;
        let effect_index = self.validate_effect_handle(effect, module)?;
        if cancellation.is_cancelled() {
            return Err(LifecycleError::TeardownCancelled {
                module_id: module.module_id.clone(),
            });
        }
        let next_generation = self
            .generation
            .checked_add(1)
            .ok_or(LifecycleError::GenerationExhausted)?;
        let mut record = self
            .modules
            .remove(&module.module_id)
            .ok_or(LifecycleError::StaleHandle(HandleKind::Module))?;
        let label = record.effects[effect_index].label.clone();

        match dispose_record(&mut record.effects[effect_index], cancellation).await {
            Ok(()) => {
                record.effects.remove(effect_index);
                self.generation = next_generation;
                self.modules.insert(module.module_id.clone(), record);
                self.push_event(
                    self.generation,
                    module.module_id.clone(),
                    LifecycleEventKind::EffectDisposed,
                );
                Ok(self.binding())
            }
            Err(failure) => Err(self.retain_pending_cleanup(
                module,
                record,
                ModuleState::Mounted,
                next_generation,
                CleanupFailure::Effect { label, failure },
            )),
        }
    }

    fn validate_binding(&self, binding: GenerationBinding) -> Result<(), LifecycleError> {
        if binding.kernel_id != self.kernel_id {
            return Err(LifecycleError::ForeignBinding);
        }
        if binding.generation != self.generation {
            return Err(LifecycleError::StaleBinding {
                expected: self.generation,
                received: binding.generation,
            });
        }
        Ok(())
    }

    fn plan_retirement(&self, root_id: &str) -> Result<Vec<String>, LifecycleError> {
        let mut closure = BTreeSet::from([root_id.to_owned()]);
        loop {
            let prior_len = closure.len();
            for (module_id, record) in &self.modules {
                if record
                    .dependencies
                    .iter()
                    .any(|dependency| closure.contains(dependency))
                {
                    closure.insert(module_id.clone());
                }
            }
            if closure.len() == prior_len {
                break;
            }
        }

        let mut indegrees = closure
            .iter()
            .map(|module_id| (module_id.clone(), 0usize))
            .collect::<BTreeMap<_, _>>();
        let mut dependents = closure
            .iter()
            .map(|module_id| (module_id.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        for module_id in &closure {
            for dependency in &self.modules[module_id].dependencies {
                if closure.contains(dependency) {
                    *indegrees.get_mut(module_id).expect("closure indegree") += 1;
                    dependents
                        .get_mut(dependency)
                        .expect("closure dependents")
                        .push(module_id.clone());
                }
            }
        }

        let mut ready = indegrees
            .iter()
            .filter_map(|(module_id, indegree)| (*indegree == 0).then_some(module_id.clone()))
            .collect::<BTreeSet<_>>();
        let mut depths = BTreeMap::from([(root_id.to_owned(), 0usize)]);
        let mut processed = 0usize;
        while let Some(module_id) = ready.pop_first() {
            processed += 1;
            let next_depth = depths
                .get(&module_id)
                .copied()
                .unwrap_or_default()
                .saturating_add(1);
            for dependent in &dependents[&module_id] {
                depths
                    .entry(dependent.clone())
                    .and_modify(|depth| *depth = (*depth).max(next_depth))
                    .or_insert(next_depth);
                let indegree = indegrees.get_mut(dependent).expect("dependent indegree");
                *indegree -= 1;
                if *indegree == 0 {
                    ready.insert(dependent.clone());
                }
            }
        }
        if processed != closure.len() {
            return Err(LifecycleError::DependencyCycle {
                module_id: root_id.to_owned(),
            });
        }

        let mut plan = closure.into_iter().collect::<Vec<_>>();
        plan.sort_by(|left, right| {
            depths[right]
                .cmp(&depths[left])
                .then_with(|| {
                    self.modules[right]
                        .mount_generation
                        .cmp(&self.modules[left].mount_generation)
                })
                .then_with(|| left.cmp(right))
        });
        Ok(plan)
    }

    fn validate_module_handle(&self, handle: &ModuleHandle) -> Result<(), LifecycleError> {
        if handle.kernel_id != self.kernel_id {
            return Err(LifecycleError::ForeignHandle(HandleKind::Module));
        }
        let Some(record) = self.modules.get(&handle.module_id) else {
            return Err(LifecycleError::StaleHandle(HandleKind::Module));
        };
        if record.mount_generation != handle.mount_generation {
            return Err(LifecycleError::StaleHandle(HandleKind::Module));
        }
        Ok(())
    }

    fn validate_disposer_handle(
        &self,
        disposer: &DisposerHandle,
        module: &ModuleHandle,
    ) -> Result<(), LifecycleError> {
        if disposer.kernel_id != self.kernel_id {
            return Err(LifecycleError::ForeignHandle(HandleKind::Disposer));
        }
        if disposer.module_id != module.module_id
            || disposer.mount_generation != module.mount_generation
        {
            return Err(LifecycleError::StaleHandle(HandleKind::Disposer));
        }
        Ok(())
    }

    fn validate_effect_handle(
        &self,
        effect: &EffectHandle,
        module: &ModuleHandle,
    ) -> Result<usize, LifecycleError> {
        if effect.kernel_id != self.kernel_id {
            return Err(LifecycleError::ForeignHandle(HandleKind::Effect));
        }
        if effect.module_id != module.module_id
            || effect.mount_generation != module.mount_generation
        {
            return Err(LifecycleError::StaleHandle(HandleKind::Effect));
        }
        let record = self
            .modules
            .get(&module.module_id)
            .ok_or(LifecycleError::StaleHandle(HandleKind::Effect))?;
        if record.state != ModuleState::Mounted {
            return Err(LifecycleError::ModuleCleanupPending {
                module_id: module.module_id.clone(),
                state: record.state,
            });
        }
        record
            .effects
            .iter()
            .position(|candidate| candidate.sequence == effect.sequence)
            .ok_or(LifecycleError::StaleHandle(HandleKind::Effect))
    }

    fn retain_pending_cleanup(
        &mut self,
        module: &ModuleHandle,
        mut record: ModuleRecord,
        original_state: ModuleState,
        generation: u64,
        failure: CleanupFailure,
    ) -> LifecycleError {
        record.state = if original_state == ModuleState::RollbackPending {
            ModuleState::RollbackPending
        } else {
            ModuleState::TeardownPending
        };
        self.generation = generation;
        self.modules.insert(module.module_id.clone(), record);
        let event = if original_state == ModuleState::RollbackPending {
            LifecycleEventKind::RollbackPending
        } else {
            LifecycleEventKind::TeardownPending
        };
        self.push_event(self.generation, module.module_id.clone(), event);
        LifecycleError::TeardownPending {
            module_id: module.module_id.clone(),
            failure,
            recovery: Box::new(self.recovery(&module.module_id, module.mount_generation)),
        }
    }

    fn recovery(&self, module_id: &str, mount_generation: u64) -> Recovery {
        Recovery {
            binding: self.binding(),
            module: ModuleHandle {
                kernel_id: self.kernel_id,
                module_id: module_id.to_owned(),
                mount_generation,
            },
            disposer: DisposerHandle {
                kernel_id: self.kernel_id,
                module_id: module_id.to_owned(),
                mount_generation,
            },
        }
    }

    fn push_event(&mut self, generation: u64, module_id: String, kind: LifecycleEventKind) {
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(LifecycleEvent {
            sequence: self.next_event_sequence,
            generation,
            module_id,
            kind,
        });
        self.next_event_sequence = self.next_event_sequence.saturating_add(1);
    }
}

fn validate_module_spec(spec: &ModuleSpec) -> Result<(), LifecycleError> {
    if !valid_id(&spec.id) {
        return Err(LifecycleError::InvalidModuleId);
    }
    if spec.dependencies.len() > MAX_DEPENDENCIES {
        return Err(LifecycleError::DependencyLimit);
    }
    let mut unique = BTreeSet::new();
    for dependency in &spec.dependencies {
        if !valid_id(dependency) {
            return Err(LifecycleError::InvalidModuleId);
        }
        if dependency == &spec.id {
            return Err(LifecycleError::SelfDependency(spec.id.clone()));
        }
        if !unique.insert(dependency) {
            return Err(LifecycleError::DuplicateDependency(dependency.clone()));
        }
    }
    Ok(())
}

fn module_snapshot(id: &str, record: &ModuleRecord) -> ModuleSnapshot {
    ModuleSnapshot {
        id: id.to_owned(),
        mount_generation: record.mount_generation,
        state: record.state,
        dependencies: record.dependencies.clone(),
        effects: record
            .effects
            .iter()
            .map(|effect| EffectSnapshot {
                sequence: effect.sequence,
                label: effect.label.clone(),
            })
            .collect(),
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-".contains(&byte))
}

fn valid_effect_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 128
        && label.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

async fn rollback_effects(
    mut effects: Vec<EffectRecord>,
) -> (Vec<EffectRecord>, Option<EffectFailure>) {
    let mut retained_reverse = Vec::new();
    let mut first_failure = None;
    let cancellation = CancellationToken::new();
    while let Some(mut effect) = effects.pop() {
        match dispose_record(&mut effect, &cancellation).await {
            Ok(()) => {}
            Err(failure) => {
                if first_failure.is_none() {
                    first_failure = Some(failure);
                }
                retained_reverse.push(effect);
            }
        }
    }
    retained_reverse.reverse();
    (retained_reverse, first_failure)
}

async fn dispose_record(
    effect: &mut EffectRecord,
    cancellation: &CancellationToken,
) -> Result<(), EffectFailure> {
    AssertUnwindSafe(effect.disposer.dispose(cancellation))
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err(EffectFailure::new("effect disposer panicked")))
}

fn bounded_failure_message(message: &str) -> String {
    let mut sanitized = String::with_capacity(message.len().min(MAX_FAILURE_BYTES));
    for character in message.chars().take(MAX_FAILURE_BYTES) {
        sanitized.push(if character.is_ascii() && !character.is_ascii_control() {
            character
        } else {
            '?'
        });
    }
    let trimmed = sanitized.trim();
    let source = if trimmed.is_empty() {
        "unspecified lifecycle failure"
    } else {
        trimmed
    };
    source[..source.len().min(MAX_FAILURE_BYTES)].to_owned()
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::{
        CleanupFailure, EffectDisposer, EffectFailure, EffectHandle, GenerationBinding, HandleKind,
        LifecycleError, LifecycleEventKind, LifecycleKernel, ModuleSpec, ModuleState, MountContext,
        MountFailure, MountFuture, MountedModule,
    };

    struct RecordingDisposer {
        label: &'static str,
        log: Arc<Mutex<Vec<String>>>,
        failures_remaining: usize,
        cancellation: Option<CancellationToken>,
    }

    impl RecordingDisposer {
        fn succeeds(label: &'static str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                label,
                log,
                failures_remaining: 0,
                cancellation: None,
            }
        }

        fn fails_once(label: &'static str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                label,
                log,
                failures_remaining: 1,
                cancellation: None,
            }
        }

        fn cancels(
            label: &'static str,
            log: Arc<Mutex<Vec<String>>>,
            token: CancellationToken,
        ) -> Self {
            Self {
                label,
                log,
                failures_remaining: 0,
                cancellation: Some(token),
            }
        }
    }

    #[async_trait]
    impl EffectDisposer for RecordingDisposer {
        async fn dispose(
            &mut self,
            _cancellation: &CancellationToken,
        ) -> Result<(), EffectFailure> {
            self.log.lock().expect("log lock").push(self.label.into());
            if let Some(cancellation) = &self.cancellation {
                cancellation.cancel();
            }
            if self.failures_remaining > 0 {
                self.failures_remaining -= 1;
                return Err(EffectFailure::new(format!(
                    "{} cleanup failed\nwith control text",
                    self.label
                )));
            }
            Ok(())
        }
    }

    struct NoopDisposer;

    #[async_trait]
    impl EffectDisposer for NoopDisposer {
        async fn dispose(
            &mut self,
            _cancellation: &CancellationToken,
        ) -> Result<(), EffectFailure> {
            Ok(())
        }
    }

    fn empty_mount(context: &mut MountContext) -> MountFuture<'_> {
        Box::pin(async move {
            assert_eq!(context.effect_count(), 0);
            Ok(())
        })
    }

    async fn mount_effects(
        kernel: &mut LifecycleKernel,
        module_id: &str,
        effects: Vec<(&'static str, bool)>,
        log: Arc<Mutex<Vec<String>>>,
    ) -> MountedModule {
        kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new(module_id),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        for (label, fail_once) in effects {
                            let effect_log = Arc::clone(&log);
                            context
                                .apply_effect(label, move || async move {
                                    Ok(if fail_once {
                                        RecordingDisposer::fails_once(label, effect_log)
                                    } else {
                                        RecordingDisposer::succeeds(label, effect_log)
                                    })
                                })
                                .await?;
                        }
                        Ok(())
                    })
                },
            )
            .await
            .expect("module mounted")
    }

    async fn mount_tree_module(
        kernel: &mut LifecycleKernel,
        module_id: &'static str,
        dependencies: &[&str],
        fail_once: bool,
        log: Arc<Mutex<Vec<String>>>,
    ) -> MountedModule {
        kernel
            .mount(
                kernel.binding(),
                ModuleSpec::with_dependencies(module_id, dependencies.iter().copied()),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect(format!("retire:{module_id}"), move || async move {
                                Ok(if fail_once {
                                    RecordingDisposer::fails_once(module_id, log)
                                } else {
                                    RecordingDisposer::succeeds(module_id, log)
                                })
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect("tree module mounted")
    }

    #[test]
    fn new_kernels_have_distinct_epochs_and_empty_bounded_snapshots() {
        let first = LifecycleKernel::new();
        let second = LifecycleKernel::new();

        assert_eq!(first.binding().generation(), 0);
        assert_ne!(first.binding().kernel_id(), second.binding().kernel_id());

        let snapshot = first.snapshot();
        assert_eq!(snapshot.kernel_id(), first.binding().kernel_id());
        assert_eq!(snapshot.generation(), 0);
        assert!(snapshot.modules().is_empty());
        assert!(snapshot.events().is_empty());
    }

    #[tokio::test]
    async fn successful_empty_mount_commits_one_generation_and_bound_handles() {
        let mut kernel = LifecycleKernel::new();
        let initial = kernel.binding();
        let mounted = kernel
            .mount(
                initial,
                ModuleSpec::new("alpha.module"),
                &CancellationToken::new(),
                empty_mount,
            )
            .await
            .expect("empty module mounted");

        assert_eq!(mounted.binding().generation(), 1);
        assert_eq!(mounted.module().module_id(), "alpha.module");
        assert_eq!(mounted.module().mount_generation(), 1);
        assert_eq!(mounted.disposer().module_id(), "alpha.module");
        assert_eq!(kernel.binding(), mounted.binding());

        let snapshot = kernel.snapshot();
        assert_eq!(snapshot.modules().len(), 1);
        assert_eq!(snapshot.modules()[0].state(), ModuleState::Mounted);
        assert_eq!(snapshot.events().len(), 1);
        assert_eq!(snapshot.events()[0].kind(), LifecycleEventKind::Mounted);
    }

    #[tokio::test]
    async fn foreign_and_stale_bindings_fail_before_module_code_runs() {
        let mut kernel = LifecycleKernel::new();
        let foreign = LifecycleKernel::new().binding();
        let invoked = Arc::new(AtomicBool::new(false));
        let callback_flag = Arc::clone(&invoked);

        let foreign_error = kernel
            .mount(
                foreign,
                ModuleSpec::new("foreign.module"),
                &CancellationToken::new(),
                move |_| {
                    callback_flag.store(true, Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                },
            )
            .await
            .expect_err("foreign epoch rejected");
        assert_eq!(foreign_error, LifecycleError::ForeignBinding);
        assert!(!invoked.load(Ordering::SeqCst));

        let stale: GenerationBinding = kernel.binding();
        kernel
            .mount(
                stale,
                ModuleSpec::new("first.module"),
                &CancellationToken::new(),
                empty_mount,
            )
            .await
            .expect("first mount");
        let stale_error = kernel
            .mount(
                stale,
                ModuleSpec::new("stale.module"),
                &CancellationToken::new(),
                empty_mount,
            )
            .await
            .expect_err("stale generation rejected");
        assert_eq!(
            stale_error,
            LifecycleError::StaleBinding {
                expected: 1,
                received: 0,
            }
        );
    }

    #[tokio::test]
    async fn precancelled_mount_does_not_invoke_module_code() {
        let mut kernel = LifecycleKernel::new();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let invoked = Arc::new(AtomicBool::new(false));
        let callback_invoked = Arc::clone(&invoked);

        let error = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("precancelled.module"),
                &cancellation,
                move |_| {
                    callback_invoked.store(true, Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                },
            )
            .await
            .expect_err("pre-cancelled mount rejected");

        assert_eq!(
            error,
            LifecycleError::MountCancelled {
                module_id: "precancelled.module".into(),
            }
        );
        assert!(!invoked.load(Ordering::SeqCst));
        assert_eq!(kernel.binding().generation(), 0);
        assert!(kernel.snapshot().events().is_empty());
    }

    #[tokio::test]
    async fn invalid_module_metadata_and_missing_dependencies_leave_state_unchanged() {
        let invalid_ids = vec![
            String::new(),
            "Upper".into(),
            "has space".into(),
            "slash/value".into(),
            "a".repeat(65),
        ];
        for id in invalid_ids {
            let mut kernel = LifecycleKernel::new();
            let error = kernel
                .mount(
                    kernel.binding(),
                    ModuleSpec::new(id),
                    &CancellationToken::new(),
                    empty_mount,
                )
                .await
                .expect_err("invalid ID rejected");
            assert_eq!(error, LifecycleError::InvalidModuleId);
            assert_eq!(kernel.binding().generation(), 0);
        }

        let mut kernel = LifecycleKernel::new();
        let dependency_error = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::with_dependencies("dependent.module", ["missing.module"]),
                &CancellationToken::new(),
                empty_mount,
            )
            .await
            .expect_err("missing dependency rejected");
        assert_eq!(
            dependency_error,
            LifecycleError::DependencyNotMounted {
                module_id: "dependent.module".into(),
                dependency: "missing.module".into(),
            }
        );
        assert_eq!(kernel.binding().generation(), 0);
        assert!(kernel.snapshot().modules().is_empty());
    }

    #[tokio::test]
    async fn failed_setup_rolls_effects_back_in_reverse_order_without_advancing_generation() {
        let mut kernel = LifecycleKernel::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let first_log = Arc::clone(&log);
        let second_log = Arc::clone(&log);

        let error = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("rollback.module"),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("first effect", move || async move {
                                Ok(RecordingDisposer::succeeds("first", first_log))
                            })
                            .await?;
                        context
                            .apply_effect("second effect", move || async move {
                                Ok(RecordingDisposer::succeeds("second", second_log))
                            })
                            .await?;
                        Err(MountFailure::module("setup stopped"))
                    })
                },
            )
            .await
            .expect_err("setup failure returned");

        assert!(matches!(error, LifecycleError::MountFailed { .. }));
        assert_eq!(&*log.lock().expect("log lock"), &["second", "first"]);
        assert_eq!(kernel.binding().generation(), 0);
        assert!(kernel.snapshot().modules().is_empty());
        assert_eq!(
            kernel.snapshot().events()[0].kind(),
            LifecycleEventKind::RollbackComplete
        );
    }

    #[tokio::test]
    async fn failed_effect_apply_rolls_back_prior_effect_without_false_ownership() {
        let mut kernel = LifecycleKernel::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let disposer_log = Arc::clone(&log);
        let error = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("apply.failure"),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("owned", move || async move {
                                Ok(RecordingDisposer::succeeds("owned", disposer_log))
                            })
                            .await?;
                        context
                            .apply_effect("not owned", || async {
                                Err::<NoopDisposer, _>(EffectFailure::new("apply failed"))
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect_err("apply failure returned");

        assert_eq!(
            error,
            LifecycleError::MountFailed {
                module_id: "apply.failure".into(),
                failure: MountFailure::EffectApply {
                    label: "not owned".into(),
                    failure: EffectFailure::new("apply failed"),
                },
            }
        );
        assert_eq!(&*log.lock().expect("log lock"), &["owned"]);
        assert_eq!(kernel.binding().generation(), 0);
        assert!(kernel.snapshot().modules().is_empty());
    }

    #[tokio::test]
    async fn cancellation_after_effect_acquisition_rolls_back_the_retained_disposer() {
        let mut kernel = LifecycleKernel::new();
        let cancellation = CancellationToken::new();
        let cancel_from_setup = cancellation.clone();
        let log = Arc::new(Mutex::new(Vec::new()));
        let disposer_log = Arc::clone(&log);

        let error = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("cancelled.module"),
                &cancellation,
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("owned resource", move || async move {
                                cancel_from_setup.cancel();
                                Ok(RecordingDisposer::succeeds("owned", disposer_log))
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect_err("cancelled mount returned");

        assert_eq!(
            error,
            LifecycleError::MountCancelled {
                module_id: "cancelled.module".into(),
            }
        );
        assert_eq!(&*log.lock().expect("log lock"), &["owned"]);
        assert_eq!(kernel.binding().generation(), 0);
        assert!(kernel.snapshot().modules().is_empty());
    }

    #[tokio::test]
    async fn setup_panic_is_bounded_and_rolls_back_acquired_effects() {
        let mut kernel = LifecycleKernel::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let disposer_log = Arc::clone(&log);

        let error = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("panic.module"),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("panic resource", move || async move {
                                Ok(RecordingDisposer::succeeds("panic", disposer_log))
                            })
                            .await?;
                        panic!("untrusted setup panic content");
                    })
                },
            )
            .await
            .expect_err("panic converted to failure");

        assert_eq!(
            error,
            LifecycleError::MountFailed {
                module_id: "panic.module".into(),
                failure: MountFailure::Panicked,
            }
        );
        assert_eq!(&*log.lock().expect("log lock"), &["panic"]);
        assert_eq!(kernel.binding().generation(), 0);
    }

    #[tokio::test]
    async fn failed_rollback_retains_only_failed_effect_and_returns_recovery_handles() {
        let mut kernel = LifecycleKernel::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let first_log = Arc::clone(&log);
        let second_log = Arc::clone(&log);

        let error = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("recovery.module"),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("first effect", move || async move {
                                Ok(RecordingDisposer::succeeds("first", first_log))
                            })
                            .await?;
                        context
                            .apply_effect("failed effect", move || async move {
                                Ok(RecordingDisposer::fails_once("failed", second_log))
                            })
                            .await?;
                        Err(MountFailure::module("force rollback"))
                    })
                },
            )
            .await
            .expect_err("incomplete rollback returned");

        let LifecycleError::RollbackPending {
            module_id,
            failure,
            cleanup,
            recovery,
        } = error
        else {
            panic!("expected retained rollback recovery");
        };
        assert_eq!(module_id, "recovery.module");
        assert_eq!(failure, MountFailure::module("force rollback"));
        assert!(!cleanup.message().contains('\n'));
        assert!(cleanup.message().len() <= 512);
        assert_eq!(recovery.binding().generation(), 1);
        assert_eq!(recovery.module().mount_generation(), 1);
        assert_eq!(recovery.disposer().module_id(), "recovery.module");
        assert_eq!(&*log.lock().expect("log lock"), &["failed", "first"]);

        let snapshot = kernel.snapshot();
        assert_eq!(snapshot.generation(), 1);
        assert_eq!(snapshot.modules().len(), 1);
        assert_eq!(snapshot.modules()[0].state(), ModuleState::RollbackPending);
        assert_eq!(snapshot.modules()[0].effects().len(), 1);
        assert_eq!(snapshot.modules()[0].effects()[0].label(), "failed effect");
        assert_eq!(
            snapshot.events().last().expect("pending event").kind(),
            LifecycleEventKind::RollbackPending
        );

        let completed = kernel
            .unmount(
                recovery.binding(),
                recovery.module(),
                recovery.disposer(),
                &CancellationToken::new(),
            )
            .await
            .expect("rollback recovery completed");
        assert_eq!(completed.generation(), 2);
        assert!(kernel.snapshot().modules().is_empty());
        assert_eq!(
            &*log.lock().expect("log lock"),
            &["failed", "first", "failed"]
        );
        assert_eq!(
            kernel.snapshot().events().last().expect("event").kind(),
            LifecycleEventKind::RollbackComplete
        );
    }

    #[tokio::test]
    async fn unmount_disposes_effects_in_reverse_order_and_removes_the_module() {
        let mut kernel = LifecycleKernel::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let mounted = mount_effects(
            &mut kernel,
            "teardown.module",
            vec![("first", false), ("second", false), ("third", false)],
            Arc::clone(&log),
        )
        .await;

        let binding = kernel
            .unmount(
                mounted.binding(),
                mounted.module(),
                mounted.disposer(),
                &CancellationToken::new(),
            )
            .await
            .expect("module unmounted");

        assert_eq!(binding.generation(), 2);
        assert_eq!(
            &*log.lock().expect("log lock"),
            &["third", "second", "first"]
        );
        assert!(kernel.snapshot().modules().is_empty());
        assert_eq!(
            kernel.snapshot().events().last().expect("event").kind(),
            LifecycleEventKind::TeardownComplete
        );
    }

    #[tokio::test]
    async fn retire_tree_disposes_deepest_dependents_before_the_root() {
        let mut kernel = LifecycleKernel::new();
        let disposed = Arc::new(Mutex::new(Vec::new()));
        let authority = mount_tree_module(
            &mut kernel,
            "platform.authority",
            &[],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "provider.local-runtime",
            &["platform.authority"],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "stream.generation",
            &["provider.local-runtime"],
            false,
            Arc::clone(&disposed),
        )
        .await;

        let binding = kernel
            .retire_tree(
                kernel.binding(),
                authority.module(),
                &CancellationToken::new(),
            )
            .await
            .expect("dependent tree retirement");

        assert_eq!(
            disposed.lock().expect("dispose log").as_slice(),
            [
                "stream.generation",
                "provider.local-runtime",
                "platform.authority",
            ]
        );
        assert!(kernel.snapshot().modules().is_empty());
        assert_eq!(binding, kernel.binding());
    }

    #[tokio::test]
    async fn retire_tree_orders_branches_by_depth_then_descending_mount_generation() {
        let mut kernel = LifecycleKernel::new();
        let disposed = Arc::new(Mutex::new(Vec::new()));
        let authority = mount_tree_module(
            &mut kernel,
            "platform.authority",
            &[],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "provider.alpha",
            &["platform.authority"],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "provider.zeta",
            &["platform.authority"],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "stream.alpha",
            &["provider.alpha"],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "stream.zeta",
            &["provider.zeta"],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "observer.zeta",
            &["platform.authority"],
            false,
            Arc::clone(&disposed),
        )
        .await;

        kernel
            .retire_tree(
                kernel.binding(),
                authority.module(),
                &CancellationToken::new(),
            )
            .await
            .expect("branch retirement");

        assert_eq!(
            disposed.lock().expect("dispose log").as_slice(),
            [
                "stream.zeta",
                "stream.alpha",
                "observer.zeta",
                "provider.zeta",
                "provider.alpha",
                "platform.authority",
            ]
        );
    }

    #[tokio::test]
    async fn retire_tree_retry_keeps_retired_dependents_removed() {
        let mut kernel = LifecycleKernel::new();
        let disposed = Arc::new(Mutex::new(Vec::new()));
        let authority = mount_tree_module(
            &mut kernel,
            "platform.authority",
            &[],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "provider.local-runtime",
            &["platform.authority"],
            true,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "stream.generation",
            &["provider.local-runtime"],
            false,
            Arc::clone(&disposed),
        )
        .await;

        let error = kernel
            .retire_tree(
                kernel.binding(),
                authority.module(),
                &CancellationToken::new(),
            )
            .await
            .expect_err("provider cleanup remains pending");
        assert!(matches!(
            error,
            LifecycleError::TeardownPending { ref module_id, .. }
                if module_id == "provider.local-runtime"
        ));
        assert_eq!(
            disposed.lock().expect("dispose log").as_slice(),
            ["stream.generation", "provider.local-runtime"]
        );
        let pending = kernel.snapshot();
        assert_eq!(
            pending
                .modules()
                .iter()
                .map(|module| module.id())
                .collect::<Vec<_>>(),
            ["platform.authority", "provider.local-runtime"]
        );
        assert_eq!(
            pending
                .modules()
                .iter()
                .find(|module| module.id() == "provider.local-runtime")
                .expect("pending provider")
                .state(),
            ModuleState::TeardownPending
        );

        kernel
            .retire_tree(
                kernel.binding(),
                authority.module(),
                &CancellationToken::new(),
            )
            .await
            .expect("dependent tree recovery");

        assert_eq!(
            disposed.lock().expect("dispose log").as_slice(),
            [
                "stream.generation",
                "provider.local-runtime",
                "provider.local-runtime",
                "platform.authority",
            ]
        );
        assert!(kernel.snapshot().modules().is_empty());
    }

    #[tokio::test]
    async fn retire_tree_precancellation_is_an_exact_noop() {
        let mut kernel = LifecycleKernel::new();
        let disposed = Arc::new(Mutex::new(Vec::new()));
        let authority = mount_tree_module(
            &mut kernel,
            "platform.authority",
            &[],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "provider.local-runtime",
            &["platform.authority"],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "stream.generation",
            &["provider.local-runtime"],
            false,
            Arc::clone(&disposed),
        )
        .await;
        let before = kernel.snapshot();
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let error = kernel
            .retire_tree(kernel.binding(), authority.module(), &cancellation)
            .await
            .expect_err("pre-cancelled retirement rejected");

        assert_eq!(
            error,
            LifecycleError::TeardownCancelled {
                module_id: "platform.authority".into(),
            }
        );
        assert_eq!(kernel.snapshot(), before);
        assert!(disposed.lock().expect("dispose log").is_empty());
    }

    #[tokio::test]
    async fn retire_tree_rejects_an_internal_dependency_cycle_before_disposal() {
        let mut kernel = LifecycleKernel::new();
        let disposed = Arc::new(Mutex::new(Vec::new()));
        let authority = mount_tree_module(
            &mut kernel,
            "platform.authority",
            &[],
            false,
            Arc::clone(&disposed),
        )
        .await;
        mount_tree_module(
            &mut kernel,
            "provider.local-runtime",
            &["platform.authority"],
            false,
            Arc::clone(&disposed),
        )
        .await;
        kernel
            .modules
            .get_mut("platform.authority")
            .expect("authority record")
            .dependencies
            .push("provider.local-runtime".into());
        let before = kernel.snapshot();

        let error = kernel
            .retire_tree(
                kernel.binding(),
                authority.module(),
                &CancellationToken::new(),
            )
            .await
            .expect_err("dependency cycle rejected");

        assert_eq!(
            error,
            LifecycleError::DependencyCycle {
                module_id: "platform.authority".into(),
            }
        );
        assert_eq!(kernel.snapshot(), before);
        assert!(disposed.lock().expect("dispose log").is_empty());
    }

    #[tokio::test]
    async fn mounted_and_recovery_pending_dependents_block_dependency_unmount() {
        let mut kernel = LifecycleKernel::new();
        let dependency = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("dependency.module"),
                &CancellationToken::new(),
                empty_mount,
            )
            .await
            .expect("dependency mounted");
        kernel
            .mount(
                kernel.binding(),
                ModuleSpec::with_dependencies("dependent.module", ["dependency.module"]),
                &CancellationToken::new(),
                empty_mount,
            )
            .await
            .expect("dependent mounted");

        let error = kernel
            .unmount(
                kernel.binding(),
                dependency.module(),
                dependency.disposer(),
                &CancellationToken::new(),
            )
            .await
            .expect_err("dependency removal blocked");
        assert_eq!(
            error,
            LifecycleError::DependentMounted {
                module_id: "dependency.module".into(),
                dependent: "dependent.module".into(),
            }
        );
        assert_eq!(kernel.binding().generation(), 2);
    }

    #[tokio::test]
    async fn failed_teardown_retains_unattempted_effects_and_retry_runs_only_retained_work() {
        let mut kernel = LifecycleKernel::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let mounted = mount_effects(
            &mut kernel,
            "retry.module",
            vec![("first", false), ("second", true), ("third", false)],
            Arc::clone(&log),
        )
        .await;

        let error = kernel
            .unmount(
                mounted.binding(),
                mounted.module(),
                mounted.disposer(),
                &CancellationToken::new(),
            )
            .await
            .expect_err("cleanup failure retained");
        let LifecycleError::TeardownPending {
            module_id,
            failure: CleanupFailure::Effect { label, failure },
            recovery,
        } = error
        else {
            panic!("expected teardown recovery");
        };
        assert_eq!(module_id, "retry.module");
        assert_eq!(label, "second");
        assert!(failure.message().contains("cleanup failed"));
        assert_eq!(recovery.binding().generation(), 2);
        assert_eq!(&*log.lock().expect("log lock"), &["third", "second"]);
        assert_eq!(
            kernel.snapshot().modules()[0].state(),
            ModuleState::TeardownPending
        );
        assert_eq!(kernel.snapshot().modules()[0].effects().len(), 2);

        let completed = kernel
            .unmount(
                recovery.binding(),
                recovery.module(),
                recovery.disposer(),
                &CancellationToken::new(),
            )
            .await
            .expect("retry completed");
        assert_eq!(completed.generation(), 3);
        assert_eq!(
            &*log.lock().expect("log lock"),
            &["third", "second", "second", "first"]
        );
        assert!(kernel.snapshot().modules().is_empty());
    }

    #[tokio::test]
    async fn teardown_cancellation_retains_remaining_effects_but_precancellation_is_noop() {
        let mut kernel = LifecycleKernel::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let cancellation = CancellationToken::new();
        let cancel_from_disposer = cancellation.clone();
        let first_log = Arc::clone(&log);
        let second_log = Arc::clone(&log);
        let mounted = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("cancel.teardown"),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("first", move || async move {
                                Ok(RecordingDisposer::succeeds("first", first_log))
                            })
                            .await?;
                        context
                            .apply_effect("second", move || async move {
                                Ok(RecordingDisposer::cancels(
                                    "second",
                                    second_log,
                                    cancel_from_disposer,
                                ))
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect("module mounted");

        let pre_cancelled = CancellationToken::new();
        pre_cancelled.cancel();
        let pre_error = kernel
            .unmount(
                mounted.binding(),
                mounted.module(),
                mounted.disposer(),
                &pre_cancelled,
            )
            .await
            .expect_err("pre-cancelled teardown rejected");
        assert_eq!(
            pre_error,
            LifecycleError::TeardownCancelled {
                module_id: "cancel.teardown".into(),
            }
        );
        assert_eq!(kernel.binding().generation(), 1);
        assert!(log.lock().expect("log lock").is_empty());

        let interrupted = kernel
            .unmount(
                kernel.binding(),
                mounted.module(),
                mounted.disposer(),
                &cancellation,
            )
            .await
            .expect_err("mid-teardown cancellation retained");
        let LifecycleError::TeardownPending {
            failure: CleanupFailure::Cancelled,
            recovery,
            ..
        } = interrupted
        else {
            panic!("expected cancellation recovery");
        };
        assert_eq!(&*log.lock().expect("log lock"), &["second"]);
        assert_eq!(recovery.binding().generation(), 2);
        assert_eq!(kernel.snapshot().modules()[0].effects().len(), 1);

        kernel
            .unmount(
                recovery.binding(),
                recovery.module(),
                recovery.disposer(),
                &CancellationToken::new(),
            )
            .await
            .expect("cancellation recovery completed");
        assert_eq!(&*log.lock().expect("log lock"), &["second", "first"]);
    }

    #[tokio::test]
    async fn foreign_and_prior_mount_handles_are_rejected_before_cleanup() {
        let mut kernel = LifecycleKernel::new();
        let original = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("handle.module"),
                &CancellationToken::new(),
                empty_mount,
            )
            .await
            .expect("original mounted");
        let mut foreign_kernel = LifecycleKernel::new();
        let foreign = foreign_kernel
            .mount(
                foreign_kernel.binding(),
                ModuleSpec::new("foreign.module"),
                &CancellationToken::new(),
                empty_mount,
            )
            .await
            .expect("foreign mounted");

        assert_eq!(
            kernel
                .unmount(
                    kernel.binding(),
                    foreign.module(),
                    original.disposer(),
                    &CancellationToken::new(),
                )
                .await
                .expect_err("foreign module handle rejected"),
            LifecycleError::ForeignHandle(HandleKind::Module)
        );
        assert_eq!(
            kernel
                .unmount(
                    kernel.binding(),
                    original.module(),
                    foreign.disposer(),
                    &CancellationToken::new(),
                )
                .await
                .expect_err("foreign disposer rejected"),
            LifecycleError::ForeignHandle(HandleKind::Disposer)
        );

        kernel
            .unmount(
                kernel.binding(),
                original.module(),
                original.disposer(),
                &CancellationToken::new(),
            )
            .await
            .expect("original removed");
        let replacement = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("handle.module"),
                &CancellationToken::new(),
                empty_mount,
            )
            .await
            .expect("replacement mounted");

        assert_eq!(
            kernel
                .inspect_module(original.module())
                .expect_err("prior module handle rejected"),
            LifecycleError::StaleHandle(HandleKind::Module)
        );
        assert_eq!(
            kernel
                .unmount(
                    kernel.binding(),
                    replacement.module(),
                    original.disposer(),
                    &CancellationToken::new(),
                )
                .await
                .expect_err("prior disposer rejected"),
            LifecycleError::StaleHandle(HandleKind::Disposer)
        );
    }

    #[tokio::test]
    async fn explicit_effect_disposal_is_generation_bound_and_cannot_run_twice() {
        let mut kernel = LifecycleKernel::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let handles = Arc::new(Mutex::new(Vec::<EffectHandle>::new()));
        let captured_handles = Arc::clone(&handles);
        let first_log = Arc::clone(&log);
        let second_log = Arc::clone(&log);
        let mounted = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("effect.module"),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        let first = context
                            .apply_effect("first", move || async move {
                                Ok(RecordingDisposer::succeeds("first", first_log))
                            })
                            .await?;
                        let second = context
                            .apply_effect("second", move || async move {
                                Ok(RecordingDisposer::succeeds("second", second_log))
                            })
                            .await?;
                        captured_handles
                            .lock()
                            .expect("handle lock")
                            .extend([first, second]);
                        Ok(())
                    })
                },
            )
            .await
            .expect("module mounted");
        let second = handles.lock().expect("handle lock")[1].clone();

        let binding = kernel
            .dispose_effect(
                mounted.binding(),
                mounted.module(),
                &second,
                &CancellationToken::new(),
            )
            .await
            .expect("effect disposed");
        assert_eq!(binding.generation(), 2);
        assert_eq!(&*log.lock().expect("log lock"), &["second"]);
        assert_eq!(kernel.snapshot().modules()[0].effects().len(), 1);
        assert_eq!(kernel.snapshot().modules()[0].effects()[0].label(), "first");
        assert_eq!(
            kernel.snapshot().events().last().expect("event").kind(),
            LifecycleEventKind::EffectDisposed
        );

        assert_eq!(
            kernel
                .dispose_effect(
                    binding,
                    mounted.module(),
                    &second,
                    &CancellationToken::new(),
                )
                .await
                .expect_err("disposed handle is stale"),
            LifecycleError::StaleHandle(HandleKind::Effect)
        );
        assert_eq!(&*log.lock().expect("log lock"), &["second"]);
    }

    #[tokio::test]
    async fn failed_explicit_effect_disposal_enters_teardown_recovery() {
        let mut kernel = LifecycleKernel::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        let handles = Arc::new(Mutex::new(Vec::<EffectHandle>::new()));
        let captured_handles = Arc::clone(&handles);
        let disposer_log = Arc::clone(&log);
        let mounted = kernel
            .mount(
                kernel.binding(),
                ModuleSpec::new("effect.failure"),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        let handle = context
                            .apply_effect("failing", move || async move {
                                Ok(RecordingDisposer::fails_once("failing", disposer_log))
                            })
                            .await?;
                        captured_handles.lock().expect("handle lock").push(handle);
                        Ok(())
                    })
                },
            )
            .await
            .expect("module mounted");
        let handle = handles.lock().expect("handle lock")[0].clone();

        let error = kernel
            .dispose_effect(
                mounted.binding(),
                mounted.module(),
                &handle,
                &CancellationToken::new(),
            )
            .await
            .expect_err("failed effect retained");
        let LifecycleError::TeardownPending { recovery, .. } = error else {
            panic!("expected teardown recovery");
        };
        assert_eq!(recovery.binding().generation(), 2);
        assert_eq!(
            kernel.snapshot().modules()[0].state(),
            ModuleState::TeardownPending
        );
        assert_eq!(kernel.snapshot().modules()[0].effects().len(), 1);

        let blocked = kernel
            .dispose_effect(
                recovery.binding(),
                recovery.module(),
                &handle,
                &CancellationToken::new(),
            )
            .await
            .expect_err("individual disposal blocked during recovery");
        assert_eq!(
            blocked,
            LifecycleError::ModuleCleanupPending {
                module_id: "effect.failure".into(),
                state: ModuleState::TeardownPending,
            }
        );
        assert_eq!(&*log.lock().expect("log lock"), &["failing"]);

        kernel
            .unmount(
                recovery.binding(),
                recovery.module(),
                recovery.disposer(),
                &CancellationToken::new(),
            )
            .await
            .expect("recovery completed");
        assert_eq!(&*log.lock().expect("log lock"), &["failing", "failing"]);
    }

    #[tokio::test]
    async fn module_dependency_and_capacity_bounds_fail_before_setup() {
        let mut kernel = LifecycleKernel::new();
        let too_many_dependencies = (0..17)
            .map(|index| format!("dependency.{index}"))
            .collect::<Vec<_>>();
        assert_eq!(
            kernel
                .mount(
                    kernel.binding(),
                    ModuleSpec::with_dependencies("bounded.module", too_many_dependencies),
                    &CancellationToken::new(),
                    empty_mount,
                )
                .await
                .expect_err("dependency limit enforced"),
            LifecycleError::DependencyLimit
        );
        assert_eq!(
            kernel
                .mount(
                    kernel.binding(),
                    ModuleSpec::with_dependencies(
                        "duplicate.module",
                        ["dependency.one", "dependency.one"],
                    ),
                    &CancellationToken::new(),
                    empty_mount,
                )
                .await
                .expect_err("duplicate dependency rejected"),
            LifecycleError::DuplicateDependency("dependency.one".into())
        );
        assert_eq!(
            kernel
                .mount(
                    kernel.binding(),
                    ModuleSpec::with_dependencies("self.module", ["self.module"]),
                    &CancellationToken::new(),
                    empty_mount,
                )
                .await
                .expect_err("self dependency rejected"),
            LifecycleError::SelfDependency("self.module".into())
        );

        for index in 0..128 {
            kernel
                .mount(
                    kernel.binding(),
                    ModuleSpec::new(format!("module.{index}")),
                    &CancellationToken::new(),
                    empty_mount,
                )
                .await
                .expect("bounded module mounted");
        }
        assert_eq!(
            kernel
                .mount(
                    kernel.binding(),
                    ModuleSpec::new("module.overflow"),
                    &CancellationToken::new(),
                    empty_mount,
                )
                .await
                .expect_err("module limit enforced"),
            LifecycleError::ModuleLimit
        );
    }

    #[tokio::test]
    async fn effect_metadata_and_capacity_bounds_fail_before_extra_effects_apply() {
        let mut invalid_kernel = LifecycleKernel::new();
        let invoked = Arc::new(AtomicBool::new(false));
        let apply_invoked = Arc::clone(&invoked);
        let invalid = invalid_kernel
            .mount(
                invalid_kernel.binding(),
                ModuleSpec::new("invalid.effect"),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("bad\nlabel", move || async move {
                                apply_invoked.store(true, Ordering::SeqCst);
                                Ok(NoopDisposer)
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect_err("invalid effect label rejected");
        assert_eq!(
            invalid,
            LifecycleError::MountFailed {
                module_id: "invalid.effect".into(),
                failure: MountFailure::InvalidEffectLabel,
            }
        );
        assert!(!invoked.load(Ordering::SeqCst));

        let mut bounded_kernel = LifecycleKernel::new();
        let overflow = bounded_kernel
            .mount(
                bounded_kernel.binding(),
                ModuleSpec::new("effect.limit"),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        for index in 0..64 {
                            context
                                .apply_effect(format!("effect {index}"), || async {
                                    Ok(NoopDisposer)
                                })
                                .await?;
                        }
                        context
                            .apply_effect("overflow", || async { Ok(NoopDisposer) })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect_err("effect limit enforced");
        assert_eq!(
            overflow,
            LifecycleError::MountFailed {
                module_id: "effect.limit".into(),
                failure: MountFailure::EffectLimit,
            }
        );
        assert_eq!(bounded_kernel.binding().generation(), 0);
        assert!(bounded_kernel.snapshot().modules().is_empty());
    }

    #[tokio::test]
    async fn live_event_projection_drops_oldest_entries_at_its_bound() {
        let mut kernel = LifecycleKernel::new();
        for _ in 0..130 {
            let mounted = kernel
                .mount(
                    kernel.binding(),
                    ModuleSpec::new("event.module"),
                    &CancellationToken::new(),
                    empty_mount,
                )
                .await
                .expect("event module mounted");
            kernel
                .unmount(
                    kernel.binding(),
                    mounted.module(),
                    mounted.disposer(),
                    &CancellationToken::new(),
                )
                .await
                .expect("event module unmounted");
        }

        let snapshot = kernel.snapshot();
        assert_eq!(snapshot.events().len(), 256);
        assert_eq!(snapshot.events()[0].sequence(), 5);
        assert_eq!(snapshot.events()[255].sequence(), 260);
    }

    #[test]
    fn failure_messages_are_control_free_and_bounded_to_512_bytes() {
        let failure = EffectFailure::new(format!("\n{}\t", "é".repeat(600)));
        assert!(failure.message().len() <= 512);
        assert!(!failure.message().chars().any(char::is_control));
        assert!(failure.message().is_ascii());
        assert!(!failure.message().is_empty());
    }
}
