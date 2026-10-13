use argentum_domain::extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust};
use argentum_domain::Sha256Digest;
use std::collections::BTreeMap;

#[test]
fn desktop_authority_is_never_inferred_from_the_existing_browser_trust() {
    let definition = ExtensionDefinition {
        namespace: "owned-native.stdio".into(),
        package_root: "invalid".into(),
        package_sha256: Sha256Digest("0".repeat(64)),
        source_id: "offline-control".into(),
        source_revision: "1".into(),
        transport: ExtensionTransport::Stdio {
            program: "invalid".into(),
            program_sha256: Sha256Digest("0".repeat(64)),
            args: vec![],
            cwd: "invalid".into(),
            environment: BTreeMap::new(),
            inputs: BTreeMap::new(),
        },
        allowed_tools: vec![],
        resource_prefixes: vec![],
        trust: ExtensionTrust::ReviewedWindowsApplication,
        image_scope: None,
    };
    let workspace = tempfile::tempdir().unwrap();
    let error = match argentum_extensions::configuration::admit(&definition, workspace.path()) {
        Ok(_) => panic!("invalid desktop declaration must not be admitted"),
        Err(error) => error.to_string(),
    };
    if !cfg!(feature = "desktop-control") || !cfg!(windows) {
        assert!(error.contains("windows_application_control_not_implemented_in_this_build"));
    } else {
        assert!(error.contains("absolute_owned_package_root_required"));
    }
    assert_ne!(
        ExtensionTrust::ReviewedWindowsApplication,
        ExtensionTrust::ReviewedCurrentUserCode
    );
}
