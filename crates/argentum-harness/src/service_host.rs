//! Lifecycle owner for trusted, in-process service definitions.
//! Installation registers metadata only; package loading and authority remain
//! the embedding host's responsibility.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use argentum_lifecycle::{
    DisposerHandle, LifecycleError, LifecycleKernel, ModuleHandle, ModuleSpec, ModuleState,
    MountContext, MountFuture,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::{
    services::valid_id, ResolvedComposition, ServiceCatalog, ServiceCompositionError, ServiceModule,
};

#[derive(Debug, Error)]
pub enum ServiceHostError {
    #[error(transparent)]
    Composition(#[from] ServiceCompositionError),
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error("module is already installed")]
    AlreadyInstalled,
    #[error("module is active or awaiting cleanup")]
    ActiveModule,
    #[error("active runs must finish before changing composition")]
    RunsActive,
    #[error("module cleanup must finish before admitting work")]
    CleanupPending,
    #[error("existing consumer must be disabled before changing its provider")]
    ReconfigurationRequired,
    #[error("active run limit reached")]
    RunLimit,
    #[error("service host is shutting down")]
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceModuleStatus {
    pub id: String,
    /// None means installed but disabled. Pending cleanup is never disabled.
    pub state: Option<ModuleState>,
}

/// Keeps a composition stable until a run releases its lease.
#[derive(Debug)]
#[must_use = "keep the lease alive for the entire run"]
pub struct ServiceRunLease {
    active: Arc<AtomicUsize>,
}

impl Drop for ServiceRunLease {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

struct OwnedModule {
    module: ModuleHandle,
    disposer: DisposerHandle,
}

pub struct ServiceHost {
    installed: BTreeMap<String, ServiceModule>,
    owned: BTreeMap<String, OwnedModule>,
    selections: BTreeMap<String, String>,
    lifecycle: LifecycleKernel,
    runs: Arc<AtomicUsize>,
    closed: bool,
}

impl Default for ServiceHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceHost {
    pub fn new() -> Self {
        Self {
            installed: BTreeMap::new(),
            owned: BTreeMap::new(),
            selections: BTreeMap::new(),
            lifecycle: LifecycleKernel::new(),
            runs: Arc::new(AtomicUsize::new(0)),
            closed: false,
        }
    }

    /// Register a trusted definition. This never opens files or executes setup.
    pub fn install(&mut self, module: ServiceModule) -> Result<(), ServiceHostError> {
        self.ensure_open()?;
        if self.installed.contains_key(&module.id) {
            return Err(ServiceHostError::AlreadyInstalled);
        }
        let mut definitions: Vec<_> = self.installed.values().cloned().collect();
        definitions.push(module.clone());
        ServiceCatalog::new(definitions)?;
        self.installed.insert(module.id.clone(), module);
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<ServiceModuleStatus> {
        let lifecycle = self.lifecycle.snapshot();
        self.installed
            .keys()
            .map(|id| ServiceModuleStatus {
                id: id.clone(),
                state: lifecycle
                    .modules()
                    .iter()
                    .find(|module| module.id() == id)
                    .map(|module| module.state()),
            })
            .collect()
    }

    pub fn begin_run(&self) -> Result<ServiceRunLease, ServiceHostError> {
        self.ensure_open()?;
        self.ensure_no_cleanup()?;
        self.runs
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 1024).then_some(n + 1)
            })
            .map_err(|_| ServiceHostError::RunLimit)?;
        Ok(ServiceRunLease {
            active: self.runs.clone(),
        })
    }

    pub async fn enable_with_setup<F>(
        &mut self,
        id: &str,
        selections: &BTreeMap<String, String>,
        cancellation: &CancellationToken,
        setup: F,
    ) -> Result<(), ServiceHostError>
    where
        F: for<'a> FnOnce(&'a mut MountContext) -> MountFuture<'a>,
    {
        self.enable_with_bindings(id, selections, cancellation, move |context, _| {
            setup(context)
        })
        .await
    }

    /// Supply the exact immutable resolution used for lifecycle admission to
    /// setup, so consumers cannot accidentally choose a different backend.
    pub async fn enable_with_bindings<F>(
        &mut self,
        id: &str,
        selections: &BTreeMap<String, String>,
        cancellation: &CancellationToken,
        setup: F,
    ) -> Result<(), ServiceHostError>
    where
        F: for<'a> FnOnce(&'a mut MountContext, ResolvedComposition) -> MountFuture<'a>,
    {
        self.ensure_open()?;
        self.ensure_idle()?;
        self.ensure_no_cleanup()?;
        self.require_installed(id)?;
        if self.owned.contains_key(id) {
            return Err(ServiceHostError::ActiveModule);
        }
        let catalog = ServiceCatalog::new(self.installed.values().cloned().collect())?;
        let mut enabled: BTreeSet<_> = self.owned.keys().cloned().collect();
        enabled.insert(id.into());
        let mut selected = self.selections.clone();
        selected.extend(selections.clone());
        let resolved = catalog.resolve(&enabled, &selected)?;
        let snapshot = self.lifecycle.snapshot();
        for active in snapshot.modules() {
            if resolved.module_dependencies(active.id()) != Some(active.dependencies()) {
                return Err(ServiceHostError::ReconfigurationRequired);
            }
        }
        let spec = ModuleSpec::with_dependencies(
            id,
            resolved
                .module_dependencies(id)
                .unwrap_or_default()
                .iter()
                .cloned(),
        );
        let setup_bindings = resolved.clone();
        match self
            .lifecycle
            .mount(
                self.lifecycle.binding(),
                spec,
                cancellation,
                move |context| setup(context, setup_bindings),
            )
            .await
        {
            Ok(mounted) => {
                self.owned.insert(
                    id.into(),
                    OwnedModule {
                        module: mounted.module().clone(),
                        disposer: mounted.disposer().clone(),
                    },
                );
                // Remember inferred bindings too, so adding an alternative does
                // not silently move existing consumers to it.
                for definition in self
                    .installed
                    .values()
                    .filter(|definition| enabled.contains(&definition.id))
                {
                    for requirement in &definition.requires {
                        if let Some(provider) = resolved.provider(&requirement.contract.id) {
                            selected.insert(requirement.contract.id.clone(), provider.into());
                        }
                    }
                }
                self.selections = selected;
                Ok(())
            }
            Err(error) => {
                if let LifecycleError::RollbackPending { recovery, .. } = &error {
                    self.owned.insert(
                        id.into(),
                        OwnedModule {
                            module: recovery.module().clone(),
                            disposer: recovery.disposer().clone(),
                        },
                    );
                }
                Err(error.into())
            }
        }
    }

    /// Dispose owned resources. Dependents must be disabled first. A failed
    /// teardown retains its handles, so this same method performs the retry.
    pub async fn disable(
        &mut self,
        id: &str,
        cancellation: &CancellationToken,
    ) -> Result<(), ServiceHostError> {
        self.ensure_idle()?;
        self.require_installed(id)?;
        let Some(owned) = self.owned.get(id) else {
            return Ok(());
        };
        self.lifecycle
            .unmount(
                self.lifecycle.binding(),
                &owned.module,
                &owned.disposer,
                cancellation,
            )
            .await?;
        self.owned.remove(id);
        self.selections.retain(|_, provider| provider != id);
        Ok(())
    }

    /// Remove only a disabled definition. Retained files and user data are not
    /// deleted by this metadata operation.
    pub fn remove(&mut self, id: &str) -> Result<(), ServiceHostError> {
        self.require_installed(id)?;
        if self.owned.contains_key(id) {
            return Err(ServiceHostError::ActiveModule);
        }
        self.installed.remove(id);
        Ok(())
    }

    /// Close run admission before checking leases, then dispose dependents
    /// first. On failure, retain the remaining owners for the same call to retry.
    pub async fn shutdown(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<(), ServiceHostError> {
        self.closed = true;
        self.ensure_idle()?;
        while !self.owned.is_empty() {
            let snapshot = self.lifecycle.snapshot();
            let leaf = snapshot
                .modules()
                .iter()
                .find(|module| {
                    !snapshot
                        .modules()
                        .iter()
                        .any(|other| other.dependencies().iter().any(|id| id == module.id()))
                })
                .map(|module| module.id().to_owned())
                .ok_or(ServiceCompositionError::DependencyCycle)?;
            self.disable(&leaf, cancellation).await?;
        }
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), ServiceHostError> {
        if self.closed {
            return Err(ServiceHostError::Closed);
        }
        Ok(())
    }

    fn require_installed(&self, id: &str) -> Result<(), ServiceHostError> {
        if !valid_id(id) {
            return Err(ServiceCompositionError::InvalidDeclaration.into());
        }
        if !self.installed.contains_key(id) {
            return Err(ServiceCompositionError::UnknownModule(id.into()).into());
        }
        Ok(())
    }

    fn ensure_idle(&self) -> Result<(), ServiceHostError> {
        if self.runs.load(Ordering::Acquire) != 0 {
            return Err(ServiceHostError::RunsActive);
        }
        Ok(())
    }

    fn ensure_no_cleanup(&self) -> Result<(), ServiceHostError> {
        if self
            .lifecycle
            .snapshot()
            .modules()
            .iter()
            .any(|module| module.state() != ModuleState::Mounted)
        {
            return Err(ServiceHostError::CleanupPending);
        }
        Ok(())
    }
}
