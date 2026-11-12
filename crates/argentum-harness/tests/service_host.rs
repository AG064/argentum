use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use argentum_harness::{
    ServiceContract, ServiceHost, ServiceHostError, ServiceModule, ServiceRequirement,
};
use argentum_lifecycle::{EffectDisposer, EffectFailure, ModuleState};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn setup_receives_the_exact_resolved_backend_binding() {
    let mut host = ServiceHost::new();
    for module in [
        definition("llama", &["backend"], &[]),
        definition("other", &["backend"], &[]),
        definition("model", &["model"], &["backend"]),
    ] {
        host.install(module).unwrap();
    }
    for id in ["llama", "other"] {
        host.enable_with_setup(id, &BTreeMap::new(), &CancellationToken::new(), |_| {
            Box::pin(async { Ok(()) })
        })
        .await
        .unwrap();
    }
    let selected = BTreeMap::from([("backend".into(), "other".into())]);
    host.enable_with_bindings(
        "model",
        &selected,
        &CancellationToken::new(),
        |_, bindings| {
            Box::pin(async move {
                assert_eq!(bindings.provider("backend"), Some("other"));
                assert_eq!(bindings.module_dependencies("model").unwrap(), &["other"]);
                Ok(())
            })
        },
    )
    .await
    .unwrap();
    host.shutdown(&CancellationToken::new()).await.unwrap();
}

fn definition(id: &str, offers: &[&str], needs: &[&str]) -> ServiceModule {
    ServiceModule {
        id: id.into(),
        provides: offers
            .iter()
            .map(|id| ServiceContract {
                id: (*id).into(),
                version: 1,
            })
            .collect(),
        requires: needs
            .iter()
            .map(|id| ServiceRequirement {
                contract: ServiceContract {
                    id: (*id).into(),
                    version: 1,
                },
                optional: false,
            })
            .collect(),
    }
}

struct Resource {
    label: &'static str,
    active: Arc<Mutex<Vec<&'static str>>>,
    fail_once: bool,
}

#[async_trait]
impl EffectDisposer for Resource {
    async fn dispose(&mut self, _: &CancellationToken) -> Result<(), EffectFailure> {
        if self.fail_once {
            self.fail_once = false;
            return Err(EffectFailure::new("resource still busy"));
        }
        self.active.lock().unwrap().retain(|id| *id != self.label);
        Ok(())
    }
}

#[tokio::test]
async fn install_is_dormant_and_disable_disposes_owned_resources_before_remove() {
    let mut host = ServiceHost::new();
    host.install(definition("llama", &["backend"], &[]))
        .unwrap();
    host.install(definition("model", &["model"], &["backend"]))
        .unwrap();
    assert!(host.snapshot().iter().all(|item| item.state.is_none()));
    let active = Arc::new(Mutex::new(Vec::new()));
    for id in ["llama", "model"] {
        let active = active.clone();
        host.enable_with_setup(
            id,
            &BTreeMap::new(),
            &CancellationToken::new(),
            move |ctx| {
                Box::pin(async move {
                    ctx.apply_effect(id, || async move {
                        active.lock().unwrap().push(id);
                        Ok(Resource {
                            label: id,
                            active,
                            fail_once: false,
                        })
                    })
                    .await?;
                    Ok(())
                })
            },
        )
        .await
        .unwrap();
    }
    assert_eq!(*active.lock().unwrap(), vec!["llama", "model"]);
    assert!(host.remove("llama").is_err());
    assert!(host
        .disable("llama", &CancellationToken::new())
        .await
        .is_err());
    assert_eq!(active.lock().unwrap().len(), 2);
    host.disable("model", &CancellationToken::new())
        .await
        .unwrap();
    host.disable("llama", &CancellationToken::new())
        .await
        .unwrap();
    assert!(active.lock().unwrap().is_empty());
    host.remove("llama").unwrap();
    assert_eq!(host.snapshot().len(), 1);
    assert_eq!(host.snapshot()[0].id, "model");
}

#[tokio::test]
async fn live_run_prevents_composition_mutation_until_lease_is_released() {
    let mut host = ServiceHost::new();
    host.install(definition("chat", &["chat"], &[])).unwrap();
    host.enable_with_setup("chat", &BTreeMap::new(), &CancellationToken::new(), |_| {
        Box::pin(async { Ok(()) })
    })
    .await
    .unwrap();
    let run = host.begin_run().unwrap();
    assert!(matches!(
        host.disable("chat", &CancellationToken::new()).await,
        Err(ServiceHostError::RunsActive)
    ));
    drop(run);
    host.disable("chat", &CancellationToken::new())
        .await
        .unwrap();
    host.enable_with_setup("chat", &BTreeMap::new(), &CancellationToken::new(), |_| {
        Box::pin(async { Ok(()) })
    })
    .await
    .unwrap();
    assert_eq!(host.snapshot()[0].state, Some(ModuleState::Mounted));
    host.disable("chat", &CancellationToken::new())
        .await
        .unwrap();
}

#[tokio::test]
async fn failed_teardown_retains_resource_and_blocks_runs_until_retry() {
    let mut host = ServiceHost::new();
    host.install(definition("logger", &["logging"], &[]))
        .unwrap();
    let active = Arc::new(Mutex::new(vec!["logger"]));
    let resource = Resource {
        label: "logger",
        active: active.clone(),
        fail_once: true,
    };
    host.enable_with_setup(
        "logger",
        &BTreeMap::new(),
        &CancellationToken::new(),
        |ctx| {
            Box::pin(async move {
                ctx.apply_effect("writer", || async { Ok(resource) })
                    .await?;
                Ok(())
            })
        },
    )
    .await
    .unwrap();
    assert!(host
        .disable("logger", &CancellationToken::new())
        .await
        .is_err());
    assert_eq!(host.snapshot()[0].state, Some(ModuleState::TeardownPending));
    assert!(host.begin_run().is_err());
    assert!(host.remove("logger").is_err());
    host.disable("logger", &CancellationToken::new())
        .await
        .unwrap();
    assert!(active.lock().unwrap().is_empty());
    assert!(host.begin_run().is_ok());
}

#[tokio::test]
async fn setup_rollback_recovery_is_retained_for_explicit_cleanup() {
    let mut host = ServiceHost::new();
    host.install(definition("logger", &["logging"], &[]))
        .unwrap();
    let active = Arc::new(Mutex::new(vec!["logger"]));
    let resource = Resource {
        label: "logger",
        active: active.clone(),
        fail_once: true,
    };
    let failed = host
        .enable_with_setup(
            "logger",
            &BTreeMap::new(),
            &CancellationToken::new(),
            |ctx| {
                Box::pin(async move {
                    ctx.apply_effect("writer", || async { Ok(resource) })
                        .await?;
                    Err(argentum_lifecycle::MountFailure::module("setup rejected"))
                })
            },
        )
        .await;
    assert!(failed.is_err());
    assert_eq!(host.snapshot()[0].state, Some(ModuleState::RollbackPending));
    assert!(host.begin_run().is_err());
    host.disable("logger", &CancellationToken::new())
        .await
        .unwrap();
    assert!(active.lock().unwrap().is_empty());
}

#[tokio::test]
async fn missing_dependency_and_precancellation_never_run_setup() {
    let mut host = ServiceHost::new();
    host.install(definition("model", &["model"], &["backend"]))
        .unwrap();
    assert!(host
        .enable_with_setup("model", &BTreeMap::new(), &CancellationToken::new(), |_| {
            Box::pin(async { panic!("invalid setup ran") })
        })
        .await
        .is_err());
    host.install(definition("backend", &["backend"], &[]))
        .unwrap();
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(host
        .enable_with_setup("backend", &BTreeMap::new(), &cancelled, |_| Box::pin(
            async { panic!("cancelled setup ran") }
        ))
        .await
        .is_err());
    assert!(host.snapshot().iter().all(|item| item.state.is_none()));
}

#[tokio::test]
async fn backend_rebinding_requires_consumer_teardown_and_preserves_unrelated_module() {
    let mut host = ServiceHost::new();
    for definition in [
        definition("llama", &["backend"], &[]),
        definition("other", &["backend"], &[]),
        definition("model", &["model"], &["backend"]),
        definition("chat", &["chat"], &[]),
    ] {
        host.install(definition).unwrap();
    }
    for id in ["chat", "llama", "model"] {
        host.enable_with_setup(id, &BTreeMap::new(), &CancellationToken::new(), |_| {
            Box::pin(async { Ok(()) })
        })
        .await
        .unwrap();
    }
    let replacement = BTreeMap::from([("backend".into(), "other".into())]);
    assert!(matches!(
        host.enable_with_setup("other", &replacement, &CancellationToken::new(), |_| {
            Box::pin(async { panic!("rebind ran before teardown") })
        })
        .await,
        Err(ServiceHostError::ReconfigurationRequired)
    ));
    host.disable("model", &CancellationToken::new())
        .await
        .unwrap();
    host.disable("llama", &CancellationToken::new())
        .await
        .unwrap();
    host.enable_with_setup("other", &replacement, &CancellationToken::new(), |_| {
        Box::pin(async { Ok(()) })
    })
    .await
    .unwrap();
    host.enable_with_setup("model", &BTreeMap::new(), &CancellationToken::new(), |_| {
        Box::pin(async { Ok(()) })
    })
    .await
    .unwrap();
    assert_eq!(
        host.snapshot()
            .iter()
            .find(|m| m.id == "chat")
            .unwrap()
            .state,
        Some(ModuleState::Mounted)
    );
    host.shutdown(&CancellationToken::new()).await.unwrap();
    assert!(host.snapshot().iter().all(|m| m.state.is_none()));
}

#[tokio::test]
async fn shutdown_closes_admission_and_retries_after_active_run_drains() {
    let mut host = ServiceHost::new();
    host.install(definition("chat", &["chat"], &[])).unwrap();
    host.enable_with_setup("chat", &BTreeMap::new(), &CancellationToken::new(), |_| {
        Box::pin(async { Ok(()) })
    })
    .await
    .unwrap();
    let lease = host.begin_run().unwrap();
    assert!(matches!(
        host.shutdown(&CancellationToken::new()).await,
        Err(ServiceHostError::RunsActive)
    ));
    assert!(matches!(host.begin_run(), Err(ServiceHostError::Closed)));
    drop(lease);
    host.shutdown(&CancellationToken::new()).await.unwrap();
    assert_eq!(host.snapshot()[0].state, None);
    assert!(matches!(
        host.enable_with_setup("chat", &BTreeMap::new(), &CancellationToken::new(), |_| {
            Box::pin(async { panic!("closed setup ran") })
        })
        .await,
        Err(ServiceHostError::Closed)
    ));
    host.shutdown(&CancellationToken::new()).await.unwrap();
}
