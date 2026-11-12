use argentum_domain::ModulePreference;
use argentum_store::Store;
use std::collections::BTreeMap;

#[test]
fn module_preferences_survive_reopen_and_are_project_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let preferences = BTreeMap::from([(
        "first-party.activity".into(),
        ModulePreference {
            installed: true,
            enabled: false,
        },
    )]);
    let store = Store::open(&path).unwrap();
    store.save_module_preferences(first, &preferences).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.load_module_preferences(first).unwrap(),
        Some(preferences)
    );
    assert_eq!(store.load_module_preferences(second).unwrap(), None);
}

#[test]
fn invalid_module_preferences_do_not_replace_saved_configuration() {
    let store = Store::open_in_memory().unwrap();
    let project = uuid::Uuid::new_v4();
    let valid = BTreeMap::from([(
        "first-party.activity".into(),
        ModulePreference {
            installed: true,
            enabled: true,
        },
    )]);
    store.save_module_preferences(project, &valid).unwrap();
    let invalid = BTreeMap::from([(
        "first-party.activity".into(),
        ModulePreference {
            installed: false,
            enabled: true,
        },
    )]);
    assert!(store.save_module_preferences(project, &invalid).is_err());
    assert_eq!(store.load_module_preferences(project).unwrap(), Some(valid));
}
