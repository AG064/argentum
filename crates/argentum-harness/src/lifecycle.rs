use std::collections::{BTreeMap, BTreeSet};

use argentum_lifecycle::{
    DisposerHandle, EffectDisposer, EffectFailure, GenerationBinding, LifecycleError,
    LifecycleKernel, LifecycleSnapshot, ModuleHandle, ModuleSpec, MountContext, MountFuture,
};
use async_trait::async_trait;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::{
    first_party_review_diff_module, CapabilityRegistration, HarnessError, HarnessRegistry,
    REVIEW_DIFF_CAPABILITY_ID,
};

#[derive(Debug, Clone)]
pub struct HarnessModuleRegistration {
    pub id: String,
    pub dependencies: Vec<String>,
    pub capabilities: Vec<CapabilityRegistration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessModuleHandle {
    module: ModuleHandle,
    disposer: DisposerHandle,
    capability_ids: Vec<String>,
}

impl HarnessModuleHandle {
    pub fn module_id(&self) -> &str {
        self.module.module_id()
    }

    pub fn capability_ids(&self) -> &[String] {
        &self.capability_ids
    }

    pub const fn mount_generation(&self) -> u64 {
        self.module.mount_generation()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ComposableHarnessError {
    #[error(transparent)]
    Harness(#[from] HarnessError),
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error(
        "module '{module_id}' capability '{capability}' depends on dynamic module '{dependency_owner}' without declaring it"
    )]
    UndeclaredDynamicDependency {
        module_id: String,
        capability: String,
        dependency_owner: String,
    },
    #[error("capability '{capability}' is still required by '{dependent}'")]
    CapabilityStillReferenced {
        capability: String,
        dependent: String,
    },
    #[error("capability '{capability}' is still referenced by execution profile '{profile}'")]
    ExecutionProfileStillReferences { capability: String, profile: String },
}

#[derive(Clone)]
struct DynamicModuleRecord {
    handle: HarnessModuleHandle,
}

pub struct ComposableHarness {
    registry: HarnessRegistry,
    lifecycle: LifecycleKernel,
    modules: BTreeMap<String, DynamicModuleRecord>,
    capability_owners: BTreeMap<String, String>,
}

impl ComposableHarness {
    pub fn built_in() -> Result<Self, ComposableHarnessError> {
        Ok(Self::from_registry(HarnessRegistry::built_in()?))
    }

    pub async fn built_in_with_review_diff_module(
        cancellation: &CancellationToken,
    ) -> Result<(Self, HarnessModuleHandle), ComposableHarnessError> {
        let mut registry = HarnessRegistry::built_in()?;
        registry
            .capabilities
            .remove(REVIEW_DIFF_CAPABILITY_ID)
            .ok_or_else(|| HarnessError::UnknownCapability(REVIEW_DIFF_CAPABILITY_ID.to_owned()))?;
        registry.validate()?;

        let mut harness = Self::from_registry(registry);
        let handle = harness
            .mount_module(
                harness.binding(),
                first_party_review_diff_module(),
                cancellation,
            )
            .await?;
        Ok((harness, handle))
    }

    fn from_registry(registry: HarnessRegistry) -> Self {
        Self {
            registry,
            lifecycle: LifecycleKernel::new(),
            modules: BTreeMap::new(),
            capability_owners: BTreeMap::new(),
        }
    }

    pub const fn registry(&self) -> &HarnessRegistry {
        &self.registry
    }

    pub const fn binding(&self) -> GenerationBinding {
        self.lifecycle.binding()
    }

    pub fn lifecycle_snapshot(&self) -> LifecycleSnapshot {
        self.lifecycle.snapshot()
    }

    pub async fn mount_module(
        &mut self,
        binding: GenerationBinding,
        registration: HarnessModuleRegistration,
        cancellation: &CancellationToken,
    ) -> Result<HarnessModuleHandle, ComposableHarnessError> {
        self.mount_module_with_setup(binding, registration, cancellation, |_| {
            Box::pin(async { Ok(()) })
        })
        .await
    }

    pub async fn mount_module_with_setup<F>(
        &mut self,
        binding: GenerationBinding,
        registration: HarnessModuleRegistration,
        cancellation: &CancellationToken,
        setup: F,
    ) -> Result<HarnessModuleHandle, ComposableHarnessError>
    where
        F: for<'a> FnOnce(&'a mut MountContext) -> MountFuture<'a>,
    {
        let spec = ModuleSpec::with_dependencies(
            registration.id.clone(),
            registration.dependencies.clone(),
        );
        self.lifecycle.validate_mount(binding, &spec)?;
        self.validate_capability_reservations(&registration)?;
        self.validate_dynamic_dependencies(&registration)?;

        let mut staged = self.registry.clone();
        let capability_ids = registration
            .capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect::<Vec<_>>();
        for capability in registration.capabilities {
            staged.register_capability(capability)?;
        }
        staged.validate()?;

        let effect_capability_ids = capability_ids.clone();
        let mounted = self
            .lifecycle
            .mount_with_followup(
                binding,
                spec,
                cancellation,
                setup,
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        for capability_id in effect_capability_ids {
                            context
                                .apply_effect(format!("capability:{capability_id}"), || async {
                                    Ok(RegistrationDisposer)
                                })
                                .await?;
                        }
                        Ok(())
                    })
                },
            )
            .await?;

        let handle = HarnessModuleHandle {
            module: mounted.module().clone(),
            disposer: mounted.disposer().clone(),
            capability_ids: capability_ids.clone(),
        };
        self.registry = staged;
        for capability_id in capability_ids {
            self.capability_owners
                .insert(capability_id, registration.id.clone());
        }
        self.modules.insert(
            registration.id,
            DynamicModuleRecord {
                handle: handle.clone(),
            },
        );
        Ok(handle)
    }

    pub async fn retire_module_tree(
        &mut self,
        binding: GenerationBinding,
        root: &HarnessModuleHandle,
        cancellation: &CancellationToken,
    ) -> Result<GenerationBinding, ComposableHarnessError> {
        self.validate_lifecycle_binding(binding)?;
        self.lifecycle.inspect_module(&root.module)?;
        let root_id = root.module_id().to_owned();
        let record = self
            .modules
            .get(&root_id)
            .ok_or(LifecycleError::StaleHandle(
                argentum_lifecycle::HandleKind::Module,
            ))?;
        if record.handle != *root {
            return Err(LifecycleError::StaleHandle(argentum_lifecycle::HandleKind::Module).into());
        }
        if cancellation.is_cancelled() {
            return Err(LifecycleError::TeardownCancelled { module_id: root_id }.into());
        }

        let module_ids = self.dynamic_retirement_closure(root.module_id())?;
        let mut removed = BTreeSet::new();
        for module_id in &module_ids {
            let record = self
                .modules
                .get(module_id)
                .ok_or(LifecycleError::StaleHandle(
                    argentum_lifecycle::HandleKind::Module,
                ))?;
            removed.extend(record.handle.capability_ids.iter().cloned());
        }
        self.validate_removal_references(&removed)?;
        let mut staged = self.registry.clone();
        for capability_id in &removed {
            staged.capabilities.remove(capability_id);
        }
        staged.validate()?;
        if cancellation.is_cancelled() {
            return Err(LifecycleError::TeardownCancelled {
                module_id: root.module_id().to_owned(),
            }
            .into());
        }

        self.registry = staged;
        let result = self
            .lifecycle
            .retire_tree(binding, &root.module, cancellation)
            .await;
        self.reconcile_dynamic_state();
        result.map_err(Into::into)
    }

    pub async fn unmount_module(
        &mut self,
        binding: GenerationBinding,
        handle: &HarnessModuleHandle,
        cancellation: &CancellationToken,
    ) -> Result<GenerationBinding, ComposableHarnessError> {
        self.lifecycle.inspect_module(&handle.module)?;
        let module_id = handle.module_id().to_owned();
        let record = self
            .modules
            .get(&module_id)
            .ok_or(LifecycleError::StaleHandle(
                argentum_lifecycle::HandleKind::Module,
            ))?;
        if record.handle != *handle {
            return Err(LifecycleError::StaleHandle(argentum_lifecycle::HandleKind::Module).into());
        }

        let removed = handle
            .capability_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.validate_removal_references(&removed)?;
        let mut staged = self.registry.clone();
        for capability_id in &removed {
            staged.capabilities.remove(capability_id);
        }
        staged.validate()?;

        let next_binding = self
            .lifecycle
            .unmount(binding, &handle.module, &handle.disposer, cancellation)
            .await?;
        self.registry = staged;
        self.modules.remove(&module_id);
        for capability_id in removed {
            self.capability_owners.remove(&capability_id);
        }
        Ok(next_binding)
    }

    fn validate_dynamic_dependencies(
        &self,
        registration: &HarnessModuleRegistration,
    ) -> Result<(), ComposableHarnessError> {
        let declared = registration
            .dependencies
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for capability in &registration.capabilities {
            for dependency in &capability.dependencies {
                let Some(owner) = self.capability_owners.get(dependency) else {
                    continue;
                };
                if owner != &registration.id && !declared.contains(owner.as_str()) {
                    return Err(ComposableHarnessError::UndeclaredDynamicDependency {
                        module_id: registration.id.clone(),
                        capability: capability.id.clone(),
                        dependency_owner: owner.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_capability_reservations(
        &self,
        registration: &HarnessModuleRegistration,
    ) -> Result<(), ComposableHarnessError> {
        for capability in &registration.capabilities {
            if self.capability_owners.contains_key(&capability.id) {
                return Err(HarnessError::DuplicateRegistration(capability.id.clone()).into());
            }
        }
        Ok(())
    }

    fn validate_lifecycle_binding(&self, binding: GenerationBinding) -> Result<(), LifecycleError> {
        let current = self.lifecycle.binding();
        if binding.kernel_id() != current.kernel_id() {
            return Err(LifecycleError::ForeignBinding);
        }
        if binding.generation() != current.generation() {
            return Err(LifecycleError::StaleBinding {
                expected: current.generation(),
                received: binding.generation(),
            });
        }
        Ok(())
    }

    fn dynamic_retirement_closure(
        &self,
        root_id: &str,
    ) -> Result<BTreeSet<String>, LifecycleError> {
        let snapshot = self.lifecycle.snapshot();
        let mut closure = BTreeSet::from([root_id.to_owned()]);
        loop {
            let prior_len = closure.len();
            for module in snapshot.modules() {
                if module
                    .dependencies()
                    .iter()
                    .any(|dependency| closure.contains(dependency))
                {
                    closure.insert(module.id().to_owned());
                }
            }
            if closure.len() == prior_len {
                break;
            }
        }
        if closure
            .iter()
            .any(|module_id| !self.modules.contains_key(module_id))
        {
            return Err(LifecycleError::StaleHandle(
                argentum_lifecycle::HandleKind::Module,
            ));
        }
        Ok(closure)
    }

    fn reconcile_dynamic_state(&mut self) {
        let live_module_ids = self
            .lifecycle
            .snapshot()
            .modules()
            .iter()
            .map(|module| module.id().to_owned())
            .collect::<BTreeSet<_>>();
        self.modules
            .retain(|module_id, _| live_module_ids.contains(module_id));
        self.capability_owners.retain(|capability_id, module_id| {
            live_module_ids.contains(module_id)
                && self
                    .modules
                    .get(module_id)
                    .is_some_and(|record| record.handle.capability_ids.contains(capability_id))
        });
    }

    fn validate_removal_references(
        &self,
        removed: &BTreeSet<String>,
    ) -> Result<(), ComposableHarnessError> {
        for (dependent, capability) in &self.registry.capabilities {
            if removed.contains(dependent) {
                continue;
            }
            if let Some(dependency) = capability
                .dependencies
                .iter()
                .find(|dependency| removed.contains(*dependency))
            {
                return Err(ComposableHarnessError::CapabilityStillReferenced {
                    capability: dependency.clone(),
                    dependent: dependent.clone(),
                });
            }
        }
        for (profile_id, profile) in &self.registry.execution_profiles {
            if let Some(capability) = profile
                .capability_enabled
                .keys()
                .find(|capability| removed.contains(*capability))
            {
                return Err(ComposableHarnessError::ExecutionProfileStillReferences {
                    capability: capability.clone(),
                    profile: profile_id.clone(),
                });
            }
        }
        Ok(())
    }
}

struct RegistrationDisposer;

#[async_trait]
impl EffectDisposer for RegistrationDisposer {
    async fn dispose(&mut self, _cancellation: &CancellationToken) -> Result<(), EffectFailure> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use argentum_domain::{
        HarnessCapabilityKind, HarnessExecutionPolicy, HarnessReadiness, LayoutProfile,
    };
    use argentum_lifecycle::{LifecycleError, LifecycleEventKind, ModuleState};
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{CapabilityRegistration, HarnessError, HarnessFacts, HarnessRegistry};

    fn capability(id: &str, dependencies: &[&str]) -> CapabilityRegistration {
        CapabilityRegistration {
            id: id.into(),
            label: format!("Capability {id}"),
            kind: HarnessCapabilityKind::Agent,
            available: true,
            enabled: true,
            configurable: true,
            readiness: HarnessReadiness::Ready,
            detail: "Dynamically composed in-process test capability.".into(),
            unavailable_reason: String::new(),
            dependencies: dependencies.iter().map(|item| (*item).into()).collect(),
        }
    }

    fn snapshot(harness: &ComposableHarness) -> argentum_domain::HarnessSnapshot {
        harness.registry().snapshot(
            &LayoutProfile::default(),
            &HarnessExecutionPolicy::default(),
            &HarnessFacts::default(),
        )
    }

    fn module_projection(harness: &ComposableHarness) -> Vec<(String, HarnessModuleHandle)> {
        harness
            .modules
            .iter()
            .map(|(module_id, record)| (module_id.clone(), record.handle.clone()))
            .collect()
    }

    struct TestDisposer {
        label: &'static str,
        log: Arc<Mutex<Vec<String>>>,
        failures_remaining: usize,
    }

    impl TestDisposer {
        fn succeeds(label: &'static str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                label,
                log,
                failures_remaining: 0,
            }
        }

        fn fails_once(label: &'static str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                label,
                log,
                failures_remaining: 1,
            }
        }
    }

    #[async_trait]
    impl EffectDisposer for TestDisposer {
        async fn dispose(
            &mut self,
            _cancellation: &CancellationToken,
        ) -> Result<(), EffectFailure> {
            self.log
                .lock()
                .expect("dispose log")
                .push(self.label.into());
            if self.failures_remaining > 0 {
                self.failures_remaining -= 1;
                return Err(EffectFailure::new(format!("{} cleanup failed", self.label)));
            }
            Ok(())
        }
    }

    #[test]
    fn construction_preserves_the_exact_static_built_in_snapshot() {
        let static_registry = HarnessRegistry::built_in().expect("static registry");
        let expected = static_registry.snapshot(
            &LayoutProfile::default(),
            &HarnessExecutionPolicy::default(),
            &HarnessFacts::default(),
        );
        let harness = ComposableHarness::built_in().expect("composable harness");

        assert_eq!(snapshot(&harness), expected);
        assert_eq!(harness.binding().generation(), 0);
        assert!(harness.lifecycle_snapshot().modules().is_empty());
    }

    #[tokio::test]
    async fn capability_mount_commits_registry_and_lifecycle_state_atomically() {
        let mut harness = ComposableHarness::built_in().expect("harness");
        let mounted = harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "dynamic.module".into(),
                    dependencies: Vec::new(),
                    capabilities: vec![
                        capability("dynamic.base", &[]),
                        capability("dynamic.child", &["dynamic.base"]),
                    ],
                },
                &CancellationToken::new(),
            )
            .await
            .expect("module mounted");

        assert_eq!(mounted.module_id(), "dynamic.module");
        assert_eq!(mounted.capability_ids(), &["dynamic.base", "dynamic.child"]);
        let registry_snapshot = snapshot(&harness);
        assert!(registry_snapshot
            .capabilities
            .iter()
            .any(|item| item.id == "dynamic.base"));
        assert!(registry_snapshot
            .capabilities
            .iter()
            .any(|item| item.id == "dynamic.child"));

        let lifecycle = harness.lifecycle_snapshot();
        assert_eq!(lifecycle.generation(), 1);
        assert_eq!(lifecycle.modules().len(), 1);
        assert_eq!(lifecycle.modules()[0].state(), ModuleState::Mounted);
        assert_eq!(lifecycle.modules()[0].effects().len(), 2);
        assert_eq!(
            lifecycle.modules()[0].effects()[0].label(),
            "capability:dynamic.base"
        );
        assert_eq!(
            lifecycle.events().last().expect("event").kind(),
            LifecycleEventKind::Mounted
        );
    }

    #[tokio::test]
    async fn lifecycle_setup_effects_precede_registration_effects() {
        let mut harness = ComposableHarness::built_in().expect("harness");
        let disposed = Arc::new(Mutex::new(Vec::new()));
        let setup_log = Arc::clone(&disposed);
        let mounted = harness
            .mount_module_with_setup(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "dynamic.setup".into(),
                    dependencies: Vec::new(),
                    capabilities: vec![capability("dynamic.setup-capability", &[])],
                },
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("setup:dynamic.setup", move || async move {
                                Ok(TestDisposer::succeeds("dynamic.setup", setup_log))
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect("module mounted with setup");

        let lifecycle = harness.lifecycle_snapshot();
        assert_eq!(
            lifecycle.modules()[0]
                .effects()
                .iter()
                .map(|effect| effect.label())
                .collect::<Vec<_>>(),
            ["setup:dynamic.setup", "capability:dynamic.setup-capability"]
        );

        harness
            .unmount_module(harness.binding(), &mounted, &CancellationToken::new())
            .await
            .expect("module unmounted");
        assert_eq!(
            disposed.lock().expect("dispose log").as_slice(),
            ["dynamic.setup"]
        );
    }

    #[tokio::test]
    async fn lifecycle_tree_recovery_reserves_withdrawn_capability_ids() {
        let mut harness = ComposableHarness::built_in().expect("harness");
        let disposed = Arc::new(Mutex::new(Vec::new()));
        let authority_log = Arc::clone(&disposed);
        let authority = harness
            .mount_module_with_setup(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "platform.authority".into(),
                    dependencies: Vec::new(),
                    capabilities: Vec::new(),
                },
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("setup:platform.authority", move || async move {
                                Ok(TestDisposer::succeeds("platform.authority", authority_log))
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect("authority mounted");
        let dependent_log = Arc::clone(&disposed);
        harness
            .mount_module_with_setup(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "provider.local-runtime".into(),
                    dependencies: vec!["platform.authority".into()],
                    capabilities: vec![capability("provider.local-runtime", &[])],
                },
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("setup:provider.local-runtime", move || async move {
                                Ok(TestDisposer::fails_once(
                                    "provider.local-runtime",
                                    dependent_log,
                                ))
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect("dependent mounted");
        assert!(snapshot(&harness)
            .capabilities
            .iter()
            .any(|item| item.id == "provider.local-runtime"));

        let error = harness
            .retire_module_tree(harness.binding(), &authority, &CancellationToken::new())
            .await
            .expect_err("dependent cleanup remains pending");
        assert!(matches!(
            error,
            ComposableHarnessError::Lifecycle(LifecycleError::TeardownPending {
                ref module_id,
                ..
            }) if module_id == "provider.local-runtime"
        ));
        assert!(!snapshot(&harness)
            .capabilities
            .iter()
            .any(|item| item.id == "provider.local-runtime"));
        assert_eq!(
            disposed.lock().expect("dispose log").as_slice(),
            ["provider.local-runtime"]
        );
        assert_eq!(module_projection(&harness).len(), 2);

        let registry_before = snapshot(&harness);
        let modules_before = module_projection(&harness);
        let owners_before = harness.capability_owners.clone();
        let lifecycle_before = harness.lifecycle_snapshot();
        let binding_before = harness.binding();
        let intervening_error = harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "provider.replacement".into(),
                    dependencies: Vec::new(),
                    capabilities: vec![capability("provider.local-runtime", &[])],
                },
                &CancellationToken::new(),
            )
            .await
            .expect_err("withdrawn capability ID remains reserved");
        assert_eq!(
            intervening_error,
            ComposableHarnessError::Harness(HarnessError::DuplicateRegistration(
                "provider.local-runtime".into(),
            ))
        );
        assert_eq!(snapshot(&harness), registry_before);
        assert_eq!(module_projection(&harness), modules_before);
        assert_eq!(harness.capability_owners, owners_before);
        assert_eq!(harness.lifecycle_snapshot(), lifecycle_before);
        assert_eq!(harness.binding(), binding_before);

        harness
            .retire_module_tree(harness.binding(), &authority, &CancellationToken::new())
            .await
            .expect("tree retirement recovered");

        assert!(!snapshot(&harness)
            .capabilities
            .iter()
            .any(|item| item.id == "provider.local-runtime"));
        assert_eq!(
            disposed.lock().expect("dispose log").as_slice(),
            [
                "provider.local-runtime",
                "provider.local-runtime",
                "platform.authority",
            ]
        );
        assert!(module_projection(&harness).is_empty());
        assert!(harness.capability_owners.is_empty());
        assert!(harness.lifecycle_snapshot().modules().is_empty());
    }

    #[tokio::test]
    async fn lifecycle_tree_precancellation_preserves_every_projection() {
        let mut harness = ComposableHarness::built_in().expect("harness");
        let disposed = Arc::new(Mutex::new(Vec::new()));
        let authority_log = Arc::clone(&disposed);
        let authority = harness
            .mount_module_with_setup(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "platform.authority".into(),
                    dependencies: Vec::new(),
                    capabilities: Vec::new(),
                },
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("setup:platform.authority", move || async move {
                                Ok(TestDisposer::succeeds("platform.authority", authority_log))
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect("authority mounted");
        let dependent_log = Arc::clone(&disposed);
        harness
            .mount_module_with_setup(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "provider.local-runtime".into(),
                    dependencies: vec!["platform.authority".into()],
                    capabilities: vec![capability("provider.local-runtime", &[])],
                },
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect("setup:provider.local-runtime", move || async move {
                                Ok(TestDisposer::succeeds(
                                    "provider.local-runtime",
                                    dependent_log,
                                ))
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .expect("dependent mounted");
        let registry_before = snapshot(&harness);
        let modules_before = module_projection(&harness);
        let owners_before = harness.capability_owners.clone();
        let lifecycle_before = harness.lifecycle_snapshot();
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let error = harness
            .retire_module_tree(harness.binding(), &authority, &cancellation)
            .await
            .expect_err("pre-cancelled retirement rejected");

        assert_eq!(
            error,
            ComposableHarnessError::Lifecycle(LifecycleError::TeardownCancelled {
                module_id: "platform.authority".into(),
            })
        );
        assert_eq!(snapshot(&harness), registry_before);
        assert_eq!(module_projection(&harness), modules_before);
        assert_eq!(harness.capability_owners, owners_before);
        assert_eq!(harness.lifecycle_snapshot(), lifecycle_before);
        assert!(disposed.lock().expect("dispose log").is_empty());
    }

    #[tokio::test]
    async fn registry_validation_failure_does_not_touch_lifecycle_or_registry() {
        let mut harness = ComposableHarness::built_in().expect("harness");
        let before = snapshot(&harness);
        let error = harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "invalid.module".into(),
                    dependencies: Vec::new(),
                    capabilities: vec![capability("dynamic.invalid", &["dynamic.missing"])],
                },
                &CancellationToken::new(),
            )
            .await
            .expect_err("invalid registry rejected");

        assert!(matches!(
            error,
            ComposableHarnessError::Harness(HarnessError::MissingDependency { .. })
        ));
        assert_eq!(snapshot(&harness), before);
        assert_eq!(harness.binding().generation(), 0);
        assert!(harness.lifecycle_snapshot().modules().is_empty());
    }

    #[tokio::test]
    async fn dynamic_capability_dependencies_require_declared_module_dependencies() {
        let mut harness = ComposableHarness::built_in().expect("harness");
        harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "base.module".into(),
                    dependencies: Vec::new(),
                    capabilities: vec![capability("dynamic.base", &[])],
                },
                &CancellationToken::new(),
            )
            .await
            .expect("base mounted");

        let error = harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "child.module".into(),
                    dependencies: Vec::new(),
                    capabilities: vec![capability("dynamic.child", &["dynamic.base"])],
                },
                &CancellationToken::new(),
            )
            .await
            .expect_err("undeclared module dependency rejected");
        assert_eq!(
            error,
            ComposableHarnessError::UndeclaredDynamicDependency {
                module_id: "child.module".into(),
                capability: "dynamic.child".into(),
                dependency_owner: "base.module".into(),
            }
        );
        assert_eq!(harness.binding().generation(), 1);
        assert!(!snapshot(&harness)
            .capabilities
            .iter()
            .any(|item| item.id == "dynamic.child"));

        harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "child.module".into(),
                    dependencies: vec!["base.module".into()],
                    capabilities: vec![capability("dynamic.child", &["dynamic.base"])],
                },
                &CancellationToken::new(),
            )
            .await
            .expect("declared dependency mounted");
        assert_eq!(harness.binding().generation(), 2);
    }

    #[tokio::test]
    async fn dependent_capabilities_block_removal_then_complete_removal_is_atomic() {
        let mut harness = ComposableHarness::built_in().expect("harness");
        let base = harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "base.module".into(),
                    dependencies: Vec::new(),
                    capabilities: vec![capability("dynamic.base", &[])],
                },
                &CancellationToken::new(),
            )
            .await
            .expect("base mounted");
        let child = harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "child.module".into(),
                    dependencies: vec!["base.module".into()],
                    capabilities: vec![capability("dynamic.child", &["dynamic.base"])],
                },
                &CancellationToken::new(),
            )
            .await
            .expect("child mounted");

        let blocked = harness
            .unmount_module(harness.binding(), &base, &CancellationToken::new())
            .await
            .expect_err("referenced capability blocked");
        assert_eq!(
            blocked,
            ComposableHarnessError::CapabilityStillReferenced {
                capability: "dynamic.base".into(),
                dependent: "dynamic.child".into(),
            }
        );
        assert_eq!(harness.binding().generation(), 2);
        assert!(snapshot(&harness)
            .capabilities
            .iter()
            .any(|item| item.id == "dynamic.base"));

        harness
            .unmount_module(harness.binding(), &child, &CancellationToken::new())
            .await
            .expect("child removed");
        harness
            .unmount_module(harness.binding(), &base, &CancellationToken::new())
            .await
            .expect("base removed");
        let final_snapshot = snapshot(&harness);
        assert!(!final_snapshot
            .capabilities
            .iter()
            .any(|item| item.id.starts_with("dynamic.")));
        assert!(harness.lifecycle_snapshot().modules().is_empty());
        assert_eq!(harness.binding().generation(), 4);
    }

    #[tokio::test]
    async fn cancelled_mount_leaves_both_projections_unchanged() {
        let mut harness = ComposableHarness::built_in().expect("harness");
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let before = snapshot(&harness);
        let error = harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "cancelled.module".into(),
                    dependencies: Vec::new(),
                    capabilities: vec![capability("dynamic.cancelled", &[])],
                },
                &cancellation,
            )
            .await
            .expect_err("cancelled mount rejected");

        assert_eq!(
            error,
            ComposableHarnessError::Lifecycle(LifecycleError::MountCancelled {
                module_id: "cancelled.module".into(),
            })
        );
        assert_eq!(snapshot(&harness), before);
        assert_eq!(harness.binding().generation(), 0);
    }

    #[tokio::test]
    async fn missing_lifecycle_dependency_does_not_commit_a_valid_staged_registry() {
        let mut harness = ComposableHarness::built_in().expect("harness");
        let before = snapshot(&harness);
        let error = harness
            .mount_module(
                harness.binding(),
                HarnessModuleRegistration {
                    id: "missing.parent".into(),
                    dependencies: vec!["absent.module".into()],
                    capabilities: vec![capability("dynamic.staged", &[])],
                },
                &CancellationToken::new(),
            )
            .await
            .expect_err("missing lifecycle dependency rejected");

        assert_eq!(
            error,
            ComposableHarnessError::Lifecycle(LifecycleError::DependencyNotMounted {
                module_id: "missing.parent".into(),
                dependency: "absent.module".into(),
            })
        );
        assert_eq!(snapshot(&harness), before);
        assert_eq!(harness.binding().generation(), 0);
    }

    #[tokio::test]
    async fn first_party_review_diff_mount_rebinds_the_static_registration_atomically() {
        let static_registry = HarnessRegistry::built_in().expect("static registry");
        let expected = static_registry.snapshot(
            &LayoutProfile::default(),
            &HarnessExecutionPolicy::default(),
            &HarnessFacts::default(),
        );
        let (harness, handle) =
            ComposableHarness::built_in_with_review_diff_module(&CancellationToken::new())
                .await
                .expect("review diff module mounted");

        assert_eq!(handle.module_id(), "first-party.review-diff");
        assert_eq!(handle.capability_ids(), &["review.diff"]);
        assert_eq!(handle.mount_generation(), 1);
        assert_eq!(snapshot(&harness), expected);

        let lifecycle = harness.lifecycle_snapshot();
        assert_eq!(lifecycle.generation(), 1);
        assert_eq!(lifecycle.modules().len(), 1);
        assert_eq!(lifecycle.modules()[0].id(), "first-party.review-diff");
        assert_eq!(lifecycle.modules()[0].state(), ModuleState::Mounted);
        assert_eq!(lifecycle.modules()[0].effects().len(), 1);
        assert_eq!(
            lifecycle.modules()[0].effects()[0].label(),
            "capability:review.diff"
        );
    }

    #[tokio::test]
    async fn first_party_review_diff_unmount_removes_only_its_owned_registration() {
        let original_capability_count =
            snapshot(&ComposableHarness::built_in().expect("static fallback harness"))
                .capabilities
                .len();
        let (mut harness, handle) =
            ComposableHarness::built_in_with_review_diff_module(&CancellationToken::new())
                .await
                .expect("review diff module mounted");

        harness
            .unmount_module(harness.binding(), &handle, &CancellationToken::new())
            .await
            .expect("review diff module unmounted");

        let unmounted = snapshot(&harness);
        assert_eq!(unmounted.capabilities.len(), original_capability_count - 1);
        assert!(!unmounted
            .capabilities
            .iter()
            .any(|capability| capability.id == "review.diff"));
        for dependency in ["changes.accounting", "approval.write"] {
            assert!(unmounted
                .capabilities
                .iter()
                .any(|capability| capability.id == dependency));
        }
        assert!(harness.lifecycle_snapshot().modules().is_empty());
        assert_eq!(harness.binding().generation(), 2);

        let replacement = harness
            .mount_module(
                harness.binding(),
                crate::first_party_review_diff_module(),
                &CancellationToken::new(),
            )
            .await
            .expect("review diff module remounted");
        assert_eq!(replacement.mount_generation(), 3);
        assert_eq!(
            snapshot(&harness).capabilities.len(),
            original_capability_count
        );
        assert!(snapshot(&harness)
            .capabilities
            .iter()
            .any(|capability| capability.id == "review.diff"));
    }

    #[tokio::test]
    async fn cancelled_first_party_review_diff_mount_preserves_static_fallback() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let error = match ComposableHarness::built_in_with_review_diff_module(&cancellation).await {
            Ok(_) => panic!("cancelled first-party mount unexpectedly succeeded"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ComposableHarnessError::Lifecycle(LifecycleError::MountCancelled {
                module_id: "first-party.review-diff".into(),
            })
        );
        let fallback = ComposableHarness::built_in().expect("static fallback preserved");
        assert_eq!(fallback.binding().generation(), 0);
        assert!(fallback.lifecycle_snapshot().modules().is_empty());
        assert!(snapshot(&fallback)
            .capabilities
            .iter()
            .any(|capability| capability.id == "review.diff"));
    }
}
