use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use argentum_domain::{HarnessModuleState, ModuleAction, ModulePreference};
use argentum_harness::{EffectDisposer, EffectFailure, ModuleState, ServiceHost, ServiceModule};
use argentum_store::Store;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::RuntimeError;

pub(super) const ACTIVITY: &str = "first-party.activity";
pub(super) const TRAJECTORY: &str = "first-party.trajectory";
pub(super) const DIAGNOSTICS: &str = "first-party.diagnostics";
const MODULES: [(&str, &str); 3] = [
    (ACTIVITY, "Activity"),
    (TRAJECTORY, "Session trajectory"),
    (DIAGNOSTICS, "Session diagnostics"),
];

pub(super) struct RuntimeModules {
    pub diagnostics: super::diagnostics::DiagnosticCapture,
    pub host: ServiceHost,
    preferences: BTreeMap<String, ModulePreference>,
    gates: BTreeMap<String, Arc<AtomicBool>>,
}

struct ProjectionDisposer(
    Arc<AtomicBool>,
    Option<super::diagnostics::DiagnosticCapture>,
);

#[async_trait]
impl EffectDisposer for ProjectionDisposer {
    async fn dispose(&mut self, _: &CancellationToken) -> Result<(), EffectFailure> {
        self.0.store(false, Ordering::Release);
        if let Some(capture) = &self.1 {
            capture.disable();
        }
        Ok(())
    }
}

impl RuntimeModules {
    pub async fn restore(store: &Store, project: Uuid) -> Result<Self, RuntimeError> {
        let preferences = store.load_module_preferences(project)?.unwrap_or_default();
        let mut result = Self {
            diagnostics: super::diagnostics::DiagnosticCapture::default(),
            host: ServiceHost::new(),
            preferences,
            gates: MODULES
                .iter()
                .map(|(id, _)| ((*id).into(), Arc::new(AtomicBool::new(false))))
                .collect(),
        };
        for (id, _) in MODULES {
            let preference = result
                .preferences
                .get(id)
                .copied()
                .unwrap_or(ModulePreference {
                    installed: true,
                    enabled: false,
                });
            if let Err(error) = result.apply(id, preference).await {
                result.host.shutdown(&CancellationToken::new()).await?;
                return Err(error);
            }
            result.preferences.insert(id.into(), preference);
        }
        Ok(result)
    }

    pub fn trajectory_gate(&self) -> Arc<AtomicBool> {
        self.gates[TRAJECTORY].clone()
    }

    pub fn snapshot(&self) -> Vec<HarnessModuleState> {
        let host = self.host.snapshot();
        MODULES.iter().map(|(id, label)| {
            let state = host.iter().find(|m| m.id == *id);
            HarnessModuleState {
                id: (*id).into(), label: (*label).into(), installed: state.is_some(),
                enabled: state.is_some_and(|m| m.state == Some(ModuleState::Mounted)) && self.gates[*id].load(Ordering::Acquire),
                recovery_pending: state.is_some_and(|m| m.state.is_some_and(|s| s != ModuleState::Mounted)),
                detail: match *id {
                    ACTIVITY => "Optional live activity list. Disabling stops collection and clears visible rows.",
                    DIAGNOSTICS => "Bounded in-memory event and trace metadata. No messages, prompts, credentials, or tool payloads. Disabling clears the capture.",
                    _ => "Optional session history viewer. Saved conversations are retained when removed.",
                }.into(),
            }
        }).collect()
    }

    pub async fn change(
        &mut self,
        store: &Store,
        project: Uuid,
        id: &str,
        action: ModuleAction,
    ) -> Result<(), RuntimeError> {
        if !MODULES.iter().any(|(known, _)| *known == id) {
            return Err(RuntimeError::UnknownModule);
        }
        let previous = self.preferences[id];
        let next = match action {
            ModuleAction::Install => ModulePreference {
                installed: true,
                enabled: previous.enabled,
            },
            ModuleAction::Enable if previous.installed => ModulePreference {
                installed: true,
                enabled: true,
            },
            ModuleAction::Enable => return Err(RuntimeError::ModuleNotInstalled),
            ModuleAction::Disable => ModulePreference {
                installed: previous.installed,
                enabled: false,
            },
            ModuleAction::Remove => ModulePreference {
                installed: false,
                enabled: false,
            },
        };
        let mut desired = store
            .load_module_preferences(project)?
            .unwrap_or_else(|| self.preferences.clone());
        self.apply(id, next).await?;
        desired.insert(id.into(), next);
        if let Err(error) = store.save_module_preferences(project, &desired) {
            // No success snapshot before durable preferences exist. Restore the
            // old live composition if persistence fails.
            self.apply(id, previous).await?;
            return Err(error.into());
        }
        self.preferences = desired;
        Ok(())
    }

    async fn apply(&mut self, id: &str, preference: ModulePreference) -> Result<(), RuntimeError> {
        let installed = self.host.snapshot().iter().any(|m| m.id == id);
        let token = CancellationToken::new();
        if !preference.installed {
            if installed {
                self.host.disable(id, &token).await?;
                self.host.remove(id)?;
            }
            return Ok(());
        }
        if !installed {
            self.host.install(ServiceModule {
                id: id.into(),
                provides: vec![],
                requires: vec![],
            })?;
        }
        if preference.enabled && !self.gates[id].load(Ordering::Acquire) {
            let gate = self.gates[id].clone();
            let diagnostics = (id == DIAGNOSTICS).then(|| self.diagnostics.clone());
            self.host
                .enable_with_setup(id, &BTreeMap::new(), &token, |context| {
                    Box::pin(async move {
                        context
                            .apply_effect("projection-registration", || async move {
                                if let Some(capture) = &diagnostics {
                                    capture.enable();
                                }
                                gate.store(true, Ordering::Release);
                                Ok(ProjectionDisposer(gate, diagnostics))
                            })
                            .await?;
                        Ok(())
                    })
                })
                .await?;
        } else if !preference.enabled {
            self.host.disable(id, &token).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn module_enable_remove_and_reinstall_are_durable_and_owned() {
        let store = Store::open_in_memory().unwrap();
        let project = Uuid::new_v4();
        let mut modules = RuntimeModules::restore(&store, project).await.unwrap();
        assert!(modules.snapshot().iter().all(|m| m.installed && !m.enabled));
        modules
            .change(&store, project, TRAJECTORY, ModuleAction::Enable)
            .await
            .unwrap();
        let gate = modules.trajectory_gate();
        assert!(gate.load(Ordering::Acquire));
        let run = modules.host.begin_run().unwrap();
        assert!(modules
            .change(&store, project, TRAJECTORY, ModuleAction::Remove)
            .await
            .is_err());
        assert!(gate.load(Ordering::Acquire));
        drop(run);
        modules
            .change(&store, project, TRAJECTORY, ModuleAction::Remove)
            .await
            .unwrap();
        assert!(!gate.load(Ordering::Acquire));
        modules
            .host
            .shutdown(&CancellationToken::new())
            .await
            .unwrap();
        let mut restored = RuntimeModules::restore(&store, project).await.unwrap();
        assert!(
            !restored
                .snapshot()
                .iter()
                .find(|m| m.id == TRAJECTORY)
                .unwrap()
                .installed
        );
        assert!(restored
            .change(&store, project, TRAJECTORY, ModuleAction::Enable)
            .await
            .is_err());
        restored
            .change(&store, project, TRAJECTORY, ModuleAction::Install)
            .await
            .unwrap();
        assert!(!restored.trajectory_gate().load(Ordering::Acquire));
        restored
            .change(&store, project, TRAJECTORY, ModuleAction::Enable)
            .await
            .unwrap();
        assert!(restored.trajectory_gate().load(Ordering::Acquire));
        restored
            .host
            .shutdown(&CancellationToken::new())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn runtime_command_controls_the_real_trajectory_gate() {
        let (_dir, runtime) = super::super::tests::composed_runtime().await;
        assert!(matches!(
            runtime.trajectory_snapshot(runtime.session_id()),
            Err(RuntimeError::TrajectoryDisabled)
        ));
        runtime
            .dispatch(argentum_domain::AppCommand::SetHarnessModule {
                module_id: TRAJECTORY.into(),
                action: ModuleAction::Enable,
            })
            .await
            .unwrap();
        assert!(runtime.trajectory_snapshot(runtime.session_id()).is_ok());
        runtime
            .dispatch(argentum_domain::AppCommand::SetHarnessModule {
                module_id: TRAJECTORY.into(),
                action: ModuleAction::Disable,
            })
            .await
            .unwrap();
        assert!(matches!(
            runtime.trajectory_snapshot(runtime.session_id()),
            Err(RuntimeError::TrajectoryDisabled)
        ));
        runtime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn enabled_preferences_restore_without_inventing_install_or_authority() {
        let store = Store::open_in_memory().unwrap();
        let project = Uuid::new_v4();
        store
            .save_module_preferences(
                project,
                &BTreeMap::from([
                    (
                        TRAJECTORY.into(),
                        ModulePreference {
                            installed: true,
                            enabled: true,
                        },
                    ),
                    (
                        ACTIVITY.into(),
                        ModulePreference {
                            installed: false,
                            enabled: false,
                        },
                    ),
                ]),
            )
            .unwrap();
        let mut modules = RuntimeModules::restore(&store, project).await.unwrap();
        assert!(modules.trajectory_gate().load(Ordering::Acquire));
        assert!(
            !modules
                .snapshot()
                .iter()
                .find(|m| m.id == ACTIVITY)
                .unwrap()
                .installed
        );
        assert!(matches!(
            modules
                .change(
                    &store,
                    project,
                    "security.disable-all",
                    ModuleAction::Enable
                )
                .await,
            Err(RuntimeError::UnknownModule)
        ));
        modules
            .host
            .shutdown(&CancellationToken::new())
            .await
            .unwrap();
        assert!(!modules.trajectory_gate().load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn active_runtime_run_blocks_module_changes_and_releases_its_lease() {
        use argentum_domain::{AppCommand, SurfaceId};
        let (_dir, runtime) = super::super::tests::composed_runtime().await;
        assert!(matches!(
            runtime
                .dispatch(AppCommand::SetSurfaceVisibility {
                    surface: SurfaceId::Activity,
                    visible: true
                })
                .await,
            Err(RuntimeError::ModuleDisabled)
        ));
        let session = runtime.session_id();
        let run = Uuid::new_v4();
        runtime.acquire_session_run(session, run).unwrap();
        assert!(matches!(
            runtime
                .dispatch(AppCommand::SetHarnessModule {
                    module_id: ACTIVITY.into(),
                    action: ModuleAction::Enable
                })
                .await,
            Err(RuntimeError::ExecutionPolicyBusy)
        ));
        runtime.clear_run_state(run, session);
        runtime
            .dispatch(AppCommand::SetHarnessModule {
                module_id: ACTIVITY.into(),
                action: ModuleAction::Enable,
            })
            .await
            .unwrap();
        runtime
            .dispatch(AppCommand::SetSurfaceVisibility {
                surface: SurfaceId::Activity,
                visible: true,
            })
            .await
            .unwrap();
        assert!(
            runtime
                .harness_snapshot()
                .unwrap()
                .surfaces
                .iter()
                .find(|s| s.id == SurfaceId::Activity)
                .unwrap()
                .visible
        );
        runtime.shutdown().await.unwrap();
    }
}
