use std::collections::{BTreeMap, BTreeSet};

use argentum_harness::{ServiceCatalog, ServiceContract, ServiceModule, ServiceRequirement};

fn contract(id: &str, version: u32) -> ServiceContract {
    ServiceContract {
        id: id.into(),
        version,
    }
}

fn module(id: &str, provides: &[(&str, u32)], requires: &[(&str, u32, bool)]) -> ServiceModule {
    ServiceModule {
        id: id.into(),
        provides: provides
            .iter()
            .map(|(id, version)| contract(id, *version))
            .collect(),
        requires: requires
            .iter()
            .map(|(id, version, optional)| ServiceRequirement {
                contract: contract(id, *version),
                optional: *optional,
            })
            .collect(),
    }
}

fn enabled(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).into()).collect()
}

fn catalog() -> ServiceCatalog {
    ServiceCatalog::new(vec![
        module(
            "local-model",
            &[("model.local", 1)],
            &[("inference.backend", 1, false), ("logging", 1, true)],
        ),
        module("llama", &[("inference.backend", 1)], &[]),
        module("alternative", &[("inference.backend", 1)], &[]),
        module("logger", &[("logging", 1)], &[]),
        module("chat", &[("ui.chat", 1)], &[]),
        module(
            "model-card",
            &[("ui.model-card", 1)],
            &[("ui.chat", 1, false), ("model.local", 1, false)],
        ),
    ])
    .unwrap()
}

#[test]
fn backend_replacement_preserves_consumer_and_requires_no_logging() {
    for backend in ["llama", "alternative"] {
        let resolved = catalog()
            .resolve(&enabled(&["local-model", backend]), &BTreeMap::new())
            .unwrap();
        assert_eq!(
            resolved.module_dependencies("local-model").unwrap(),
            &[backend]
        );
        assert_eq!(resolved.mount_order(), &[backend, "local-model"]);
        assert_eq!(resolved.provider("inference.backend"), Some(backend));
        assert_eq!(resolved.provider("logging"), None);
    }
}

#[test]
fn ambiguous_provider_requires_explicit_enabled_selection() {
    let all = enabled(&["local-model", "llama", "alternative"]);
    assert!(catalog().resolve(&all, &BTreeMap::new()).is_err());
    let selected = BTreeMap::from([("inference.backend".into(), "llama".into())]);
    let resolved = catalog().resolve(&all, &selected).unwrap();
    assert_eq!(
        resolved.module_dependencies("local-model").unwrap(),
        &["llama"]
    );
    assert!(catalog()
        .resolve(&enabled(&["local-model", "alternative"]), &selected)
        .is_err());
}

#[test]
fn missing_or_incompatible_backend_is_not_activated_implicitly() {
    assert!(catalog()
        .resolve(&enabled(&["local-model"]), &BTreeMap::new())
        .is_err());
    let catalog = ServiceCatalog::new(vec![
        module("consumer", &[], &[("backend", 2, false)]),
        module("backend-v1", &[("backend", 1)], &[]),
    ])
    .unwrap();
    assert!(catalog
        .resolve(&enabled(&["consumer", "backend-v1"]), &BTreeMap::new())
        .is_err());
}

#[test]
fn nested_ui_and_optional_logging_have_exact_teardown_dependencies() {
    let resolved = catalog()
        .resolve(
            &enabled(&["chat", "local-model", "llama", "logger", "model-card"]),
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(
        resolved.module_dependencies("model-card").unwrap(),
        &["chat", "local-model"]
    );
    assert_eq!(
        resolved.removal_order("llama").unwrap(),
        vec!["model-card", "local-model", "llama"]
    );
    assert_eq!(
        resolved.removal_order("logger").unwrap(),
        vec!["model-card", "local-model", "logger"]
    );
    assert!(resolved.removal_order("absent").is_err());
}

#[test]
fn cycles_and_invalid_catalogs_are_rejected() {
    let cyclic = ServiceCatalog::new(vec![
        module("a", &[("a.service", 1)], &[("b.service", 1, false)]),
        module("b", &[("b.service", 1)], &[("a.service", 1, false)]),
    ])
    .unwrap();
    assert!(cyclic
        .resolve(&enabled(&["a", "b"]), &BTreeMap::new())
        .is_err());
    assert!(ServiceCatalog::new(vec![module("same", &[], &[]), module("same", &[], &[])]).is_err());
    assert!(ServiceCatalog::new(vec![module("../unsafe", &[], &[])]).is_err());
    assert!(ServiceCatalog::new(vec![module("valid", &[("service", 0)], &[])]).is_err());
    assert!(ServiceCatalog::new(vec![module(
        "valid",
        &[("service", 1), ("service", 2)],
        &[]
    )])
    .is_err());
    assert!(catalog()
        .resolve(&enabled(&["unknown"]), &BTreeMap::new())
        .is_err());
}

#[test]
fn oversized_catalogs_and_unknown_selections_are_rejected() {
    let oversized = (0..129)
        .map(|n| module(&format!("module-{n}"), &[], &[]))
        .collect();
    assert!(ServiceCatalog::new(oversized).is_err());
    let too_many = (0..17)
        .map(|n| contract(&format!("service-{n}"), 1))
        .collect();
    assert!(ServiceCatalog::new(vec![ServiceModule {
        id: "module".into(),
        provides: too_many,
        requires: vec![]
    }])
    .is_err());
    assert!(catalog()
        .resolve(
            &enabled(&["chat"]),
            &BTreeMap::from([("missing".into(), "chat".into())])
        )
        .is_err());
    let empty = catalog()
        .resolve(&BTreeSet::new(), &BTreeMap::new())
        .unwrap();
    assert!(empty.mount_order().is_empty());
}

#[test]
fn invalid_caller_ids_never_enter_errors() {
    use argentum_harness::ServiceCompositionError;
    for id in ["x".repeat(100_000), "bad\n\u{1b}[31m".into()] {
        assert_eq!(
            catalog()
                .resolve(&BTreeSet::from([id.clone()]), &BTreeMap::new())
                .unwrap_err(),
            ServiceCompositionError::InvalidDeclaration
        );
        assert_eq!(
            catalog()
                .resolve(
                    &enabled(&["chat"]),
                    &BTreeMap::from([(id.clone(), "chat".into())])
                )
                .unwrap_err(),
            ServiceCompositionError::InvalidDeclaration
        );
        let resolved = catalog()
            .resolve(&enabled(&["chat"]), &BTreeMap::new())
            .unwrap();
        assert_eq!(
            resolved.removal_order(&id).unwrap_err(),
            ServiceCompositionError::InvalidDeclaration
        );
    }
}

#[tokio::test]
async fn resolved_backend_dependency_drives_real_lifecycle_removal_and_replacement() {
    use argentum_harness::{ComposableHarness, HarnessModuleRegistration};
    use tokio_util::sync::CancellationToken;

    let token = CancellationToken::new();
    let mut harness = ComposableHarness::built_in().unwrap();
    let mut stale = None;
    for backend in ["llama", "alternative"] {
        let resolved = catalog()
            .resolve(&enabled(&["local-model", backend]), &BTreeMap::new())
            .unwrap();
        let mut handles = BTreeMap::new();
        for id in resolved.mount_order() {
            let handle = harness
                .mount_module(
                    harness.binding(),
                    HarnessModuleRegistration {
                        id: id.clone(),
                        dependencies: resolved.module_dependencies(id).unwrap().to_vec(),
                        capabilities: vec![],
                    },
                    &token,
                )
                .await
                .unwrap();
            handles.insert(id.clone(), handle);
        }
        assert!(harness
            .unmount_module(harness.binding(), &handles[backend], &token)
            .await
            .is_err());
        if let Some(old) = &stale {
            assert!(harness
                .unmount_module(harness.binding(), old, &token)
                .await
                .is_err());
        }
        for id in resolved.removal_order(backend).unwrap() {
            harness
                .unmount_module(harness.binding(), &handles[&id], &token)
                .await
                .unwrap();
        }
        stale = Some(handles["local-model"].clone());
        assert!(harness.lifecycle_snapshot().modules().is_empty());
    }
}
