// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

use crate::{connection::ConnectionError, package};
use argentum_domain::{
    extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust},
    Sha256Digest,
};
use argentum_execution::{ProcessSpec, VerifiedFileLease};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub struct AdmittedDefinition {
    pub definition: ExtensionDefinition,
    pub binding: Sha256Digest,
    pub program_spec: Option<ProcessSpec>,
    pub file_leases: Vec<VerifiedFileLease>,
}

impl AdmittedDefinition {
    pub fn still_current(&self) -> bool {
        self.file_leases
            .iter()
            .all(|lease| lease.verify_identity().is_ok())
    }
}

fn rejected(reason: &'static str) -> ConnectionError {
    ConnectionError::Rejected(reason)
}
fn digest_valid(digest: &Sha256Digest) -> bool {
    digest.0.len() == 64
        && digest
            .0
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

pub fn approval_binding(
    definition: &ExtensionDefinition,
    workspace: &Path,
) -> Result<Sha256Digest, ConnectionError> {
    let workspace = workspace
        .canonicalize()
        .map_err(|_| rejected("workspace_identity_unavailable"))?;
    let limits = crate::default_process_limits()?;
    let bytes = serde_json::to_vec(&(
        "argentum-extension-activation/v1",
        crate::SDK_VERSION,
        crate::MODERN_PROTOCOL,
        crate::LEGACY_PROTOCOL,
        (
            limits.active_process_limit(),
            limits.cpu_rate_percent(),
            limits.process_memory_bytes(),
            limits.job_memory_bytes(),
        ),
        definition,
        workspace,
    ))
    .map_err(|_| rejected("invalid_configuration"))?;
    Ok(Sha256Digest(format!("{:x}", Sha256::digest(bytes))))
}

pub fn admit(
    definition: &ExtensionDefinition,
    workspace: &Path,
) -> Result<AdmittedDefinition, ConnectionError> {
    if definition.trust == ExtensionTrust::ReviewedWindowsApplication
        && !crate::desktop_control_available()
    {
        return Err(rejected(
            "windows_application_control_not_implemented_in_this_build",
        ));
    }
    let parts: Vec<_> = definition.namespace.split('.').collect();
    if parts.len() != 2
        || parts.iter().any(|part| !package::valid_id(part))
        || !digest_valid(&definition.package_sha256)
        || definition.source_id.is_empty()
        || definition.source_id.len() > 2048
        || definition.source_revision.is_empty()
        || definition.source_revision.len() > 256
        || definition.allowed_tools.len() > 128
        || definition.resource_prefixes.len() > 16
        || definition
            .allowed_tools
            .iter()
            .any(|tool| tool.is_empty() || tool.len() > 128 || tool.chars().any(char::is_control))
        || definition
            .resource_prefixes
            .iter()
            .any(|uri| uri.is_empty() || uri.len() > 1024 || uri.chars().any(char::is_control))
    {
        return Err(rejected("invalid_extension_identity_or_scope"));
    }
    let unique: BTreeSet<_> = definition.allowed_tools.iter().collect();
    if let Some(scope) = &definition.image_scope {
        if !Path::new(&scope.directory).is_absolute()
            || scope.origins.is_empty()
            || scope.origins.len() > 16
            || !matches!(
                definition.trust,
                ExtensionTrust::ReviewedCurrentUserCode
                    | ExtensionTrust::ReviewedWindowsApplication
            )
        {
            return Err(rejected(
                "image_scope_requires_explicit_owned_directory_and_trusted_producer",
            ));
        }
        for origin in &scope.origins {
            if crate::http::endpoint_origin(origin).map_err(|_| rejected("invalid_image_origin"))?
                != *origin
            {
                return Err(rejected("image_scope_must_use_exact_origins"));
            }
        }
        let acl = argentum_execution::owner_only_acl_evidence(&scope.directory).map_err(|_| {
            rejected("prepare_a_dedicated_private_image_directory_before_configuration")
        })?;
        if !acl.protected
            || acl.inherited_ace_count != 0
            || acl.non_owner_allow_ace_count != 0
            || !acl.owner_full_control
        {
            return Err(rejected(
                "image_directory_must_already_be_owner_only_existing_permissions_are_not_rewritten",
            ));
        }
    }
    if unique.len() != definition.allowed_tools.len() {
        return Err(rejected("duplicate_tool_scope"));
    }
    let package_root = Path::new(&definition.package_root);
    if !package_root.is_absolute() {
        return Err(rejected("absolute_owned_package_root_required"));
    }
    let inspection =
        package::inspect(package_root).map_err(|_| rejected("package_inspection_failed"))?;
    if inspection.digest != definition.package_sha256.0 || inspection.manifest.name != parts[0] {
        return Err(rejected("package_identity_changed"));
    }
    if inspection
        .unsupported_host_components
        .iter()
        .any(|component| !component.ends_with(".interface"))
    {
        return Err(rejected(
            "required_host_hooks_or_registered_apps_unsupported_remove_or_use_their_native_host",
        ));
    }
    let mut leases = Vec::new();
    for (name, expected) in &inspection.files {
        let lease = VerifiedFileLease::open(
            package_root
                .join(package::relative_path(name).map_err(|_| rejected("invalid_package_path"))?),
            4 * 1024 * 1024,
        )
        .map_err(|_| rejected("package_file_lease_unavailable"))?;
        if lease.sha256() != expected {
            return Err(rejected("package_file_changed_during_admission"));
        }
        leases.push(lease);
    }
    let program_spec = match &definition.transport {
        ExtensionTransport::Skills => {
            if definition.trust != ExtensionTrust::ReadOnlyInstructions
                || parts[1] != "skills"
                || !definition.allowed_tools.is_empty()
                || !definition.resource_prefixes.is_empty()
                || inspection.skill_paths.is_empty()
            {
                return Err(rejected(
                    "skills_only_configuration_must_not_request_execution_or_network",
                ));
            }
            None
        }
        ExtensionTransport::Stdio {
            program,
            program_sha256,
            args,
            cwd,
            environment,
            inputs,
        } => {
            if !matches!(
                definition.trust,
                ExtensionTrust::ReviewedCurrentUserCode
                    | ExtensionTrust::ReviewedWindowsApplication
            ) || !Path::new(program).is_absolute()
                || !digest_valid(program_sha256)
                || inputs.len() > 4096
            {
                return Err(rejected(
                    "stdio_requires_exact_reviewed_current_user_code_identity",
                ));
            }
            let declaration = inspection
                .servers
                .get(parts[1])
                .ok_or_else(|| rejected("server_not_in_portable_manifest"))?;
            let package::ServerDeclaration::Stdio {
                command,
                args: declared_args,
                ..
            } = declaration
            else {
                return Err(rejected("transport_declaration_mismatch"));
            };
            let command_matches = if Path::new(command).is_absolute() {
                Path::new(command).canonicalize().ok() == Path::new(program).canonicalize().ok()
            } else {
                Path::new(command).components().count() == 1
                    && Path::new(program).file_name() == Path::new(command).file_name()
            };
            if !command_matches || declared_args != args {
                return Err(rejected(
                    "reviewed_command_or_arguments_do_not_match_pinned_manifest",
                ));
            }
            let lease = VerifiedFileLease::open(program, 256 * 1024 * 1024)
                .map_err(|_| rejected("reviewed_executable_unavailable"))?;
            if lease.sha256() != program_sha256.0 {
                return Err(rejected("executable_identity_changed"));
            }
            let program = lease.path().to_string_lossy().into_owned();
            leases.push(lease);
            let mut input_bytes = 0_u64;
            for (input, expected) in inputs {
                if !Path::new(input).is_absolute() || !digest_valid(expected) {
                    return Err(rejected("invalid_dependency_identity"));
                }
                let lease = VerifiedFileLease::open(input, 256 * 1024 * 1024)
                    .map_err(|_| rejected("dependency_file_unavailable"))?;
                input_bytes = input_bytes.saturating_add(lease.size_bytes());
                if input_bytes > 1024 * 1024 * 1024 || lease.sha256() != expected.0 {
                    return Err(rejected("dependency_identity_or_capacity_changed"));
                }
                leases.push(lease);
            }
            let cwd_path = PathBuf::from(cwd)
                .canonicalize()
                .map_err(|_| rejected("working_directory_unavailable"))?;
            if cwd_path
                != workspace
                    .canonicalize()
                    .map_err(|_| rejected("workspace_unavailable"))?
                && cwd_path
                    != package_root
                        .canonicalize()
                        .map_err(|_| rejected("package_unavailable"))?
            {
                return Err(rejected(
                    "working_directory_not_in_reviewed_package_or_workspace",
                ));
            }
            Some(
                ProcessSpec::argv(program, args.clone(), cwd_path)?
                    .with_environment(environment.clone())?,
            )
        }
        ExtensionTransport::StreamableHttp {
            endpoint,
            addresses,
            local_demo,
            credential_account,
        } => {
            if definition.trust != ExtensionTrust::ReviewedRemoteService {
                return Err(rejected("HTTP_requires_reviewed_remote_service_boundary"));
            }
            let declaration = inspection
                .servers
                .get(parts[1])
                .ok_or_else(|| rejected("server_not_in_portable_manifest"))?;
            if !matches!(declaration, package::ServerDeclaration::Http { url } if url == endpoint) {
                return Err(rejected("endpoint_does_not_match_portable_declaration"));
            }
            let addresses = addresses
                .iter()
                .map(|address| address.parse())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| rejected("invalid_reviewed_addresses"))?;
            if credential_account.as_ref().is_some_and(|account| {
                account.is_empty() || account.len() > 128 || account.chars().any(char::is_control)
            }) {
                return Err(rejected("invalid_secure_account_binding"));
            }
            crate::http::ReviewedHttp::new(endpoint, &addresses, *local_demo, None)
                .map_err(|_| rejected("HTTP_destination_review_failed"))?;
            None
        }
    };
    Ok(AdmittedDefinition {
        definition: definition.clone(),
        binding: approval_binding(definition, workspace)?,
        program_spec,
        file_leases: leases,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::extensions::ExtensionImageScope;
    use serde_json::json;
    use std::collections::BTreeMap;

    type Change = Box<dyn Fn(&mut ExtensionDefinition)>;
    type StdioEdit<'a> = &'a dyn Fn(
        &mut String,
        &mut Sha256Digest,
        &mut Vec<String>,
        &mut String,
        &mut BTreeMap<String, Sha256Digest>,
    );

    /// A portable package with a stdio server that runs this test binary, an
    /// HTTP server on loopback, and optionally a skill.
    fn package(root: &Path, name: &str, skill: bool) -> (PathBuf, String) {
        let package = root.join(name);
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(
            package.join("plugin.json"),
            serde_json::to_vec(&json!({"$schema": package::PLUGIN_SCHEMA, "name": name,
                "version": "1", "description": "test package", "license": "MIT"}))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(package.join("LICENSE"), "test license").unwrap();
        let program = std::env::current_exe().unwrap();
        std::fs::write(
            package.join("mcp.json"),
            serde_json::to_vec(&json!({"$schema": package::MCP_SCHEMA, "mcpServers": {
                "stdio": {"type": "stdio", "command": program, "args": ["--flag"]},
                "web": {"type": "streamable-http", "url": "http://127.0.0.1:18081/mcp"}}}))
            .unwrap(),
        )
        .unwrap();
        if skill {
            std::fs::create_dir_all(package.join("skills/sample")).unwrap();
            std::fs::write(package.join("skills/sample/SKILL.md"), "# Sample skill").unwrap();
        }
        let digest = package::inspect(&package).unwrap().digest;
        (package, digest)
    }

    fn program_digest() -> Sha256Digest {
        let lease =
            VerifiedFileLease::open(std::env::current_exe().unwrap(), 256 * 1024 * 1024).unwrap();
        Sha256Digest(lease.sha256().into())
    }

    fn stdio_definition(package: &Path, digest: &str, workspace: &Path) -> ExtensionDefinition {
        ExtensionDefinition {
            namespace: format!("{}.stdio", package.file_name().unwrap().to_string_lossy()),
            package_root: package.to_string_lossy().into_owned(),
            package_sha256: Sha256Digest(digest.into()),
            source_id: "test".into(),
            source_revision: "v1".into(),
            transport: ExtensionTransport::Stdio {
                program: std::env::current_exe()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                program_sha256: program_digest(),
                args: vec!["--flag".into()],
                cwd: workspace.to_string_lossy().into_owned(),
                environment: BTreeMap::new(),
                inputs: BTreeMap::new(),
            },
            allowed_tools: vec!["read".into()],
            resource_prefixes: vec![],
            trust: ExtensionTrust::ReviewedCurrentUserCode,
            image_scope: None,
        }
    }

    fn reason(result: Result<AdmittedDefinition, ConnectionError>) -> &'static str {
        match result {
            Err(ConnectionError::Rejected(reason)) => reason,
            Err(other) => panic!("unexpected error {other}"),
            Ok(_) => panic!("definition was admitted"),
        }
    }

    #[test]
    fn digests_must_be_64_lowercase_hex_characters() {
        assert!(digest_valid(&Sha256Digest("a".repeat(64))));
        assert!(!digest_valid(&Sha256Digest("A".repeat(64))));
        assert!(!digest_valid(&Sha256Digest("a".repeat(63))));
        assert!(!digest_valid(&Sha256Digest("g".repeat(64))));
    }

    #[test]
    fn a_reviewed_stdio_server_is_admitted_and_bound_to_its_workspace() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let (package, digest) = package(root.path(), "sample", false);
        let definition = stdio_definition(&package, &digest, &workspace);
        let admitted = admit(&definition, &workspace).unwrap();
        assert!(admitted.still_current());
        assert!(admitted.program_spec.is_some());
        assert_eq!(
            admitted.binding,
            approval_binding(&definition, &workspace).unwrap()
        );
        let other = root.path().join("other");
        std::fs::create_dir(&other).unwrap();
        assert_ne!(
            admitted.binding,
            approval_binding(&definition, &other).unwrap()
        );
        assert_eq!(
            reason(admit(&definition, &root.path().join("missing")).map(|_| unreachable!())),
            "workspace_unavailable"
        );
    }

    #[test]
    fn identity_and_scope_must_be_bounded_and_exact() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let (package, digest) = package(root.path(), "sample", false);
        let base = stdio_definition(&package, &digest, &workspace);
        let cases: Vec<(Change, &str)> = vec![
            (
                Box::new(|d| d.namespace = "sample".into()),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.namespace = "sample.Bad Name".into()),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.package_sha256 = Sha256Digest("x".into())),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.source_id.clear()),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.source_id = "s".repeat(2049)),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.source_revision.clear()),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.source_revision = "r".repeat(257)),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.allowed_tools = vec!["t".into(); 129]),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.allowed_tools = vec![String::new()]),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.allowed_tools = vec!["a\u{7}".into()]),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.resource_prefixes = vec!["r".into(); 17]),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.resource_prefixes = vec!["\n".into()]),
                "invalid_extension_identity_or_scope",
            ),
            (
                Box::new(|d| d.allowed_tools = vec!["read".into(), "read".into()]),
                "duplicate_tool_scope",
            ),
            (
                Box::new(|d| d.package_root = "relative/package".into()),
                "absolute_owned_package_root_required",
            ),
            (
                Box::new(|d| d.package_sha256 = Sha256Digest("0".repeat(64))),
                "package_identity_changed",
            ),
            (
                Box::new(|d| d.namespace = "renamed.stdio".into()),
                "package_identity_changed",
            ),
        ];
        for (change, expected) in cases {
            let mut definition = base.clone();
            change(&mut definition);
            assert_eq!(reason(admit(&definition, &workspace)), expected);
        }
        let mut missing = base.clone();
        missing.package_root = root.path().join("absent").to_string_lossy().into_owned();
        assert_eq!(
            reason(admit(&missing, &workspace)),
            "package_inspection_failed"
        );
    }

    #[test]
    fn stdio_needs_the_exact_reviewed_program_arguments_and_folder() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let (package, digest) = package(root.path(), "sample", false);
        let base = stdio_definition(&package, &digest, &workspace);
        let set = |definition: &mut ExtensionDefinition, f: StdioEdit| {
            if let ExtensionTransport::Stdio {
                program,
                program_sha256,
                args,
                cwd,
                inputs,
                ..
            } = &mut definition.transport
            {
                f(program, program_sha256, args, cwd, inputs);
            }
        };
        let mut wrong_trust = base.clone();
        wrong_trust.trust = ExtensionTrust::ReviewedRemoteService;
        assert_eq!(
            reason(admit(&wrong_trust, &workspace)),
            "stdio_requires_exact_reviewed_current_user_code_identity"
        );
        let mut relative = base.clone();
        set(&mut relative, &|p, _, _, _, _| *p = "argentum.exe".into());
        assert_eq!(
            reason(admit(&relative, &workspace)),
            "stdio_requires_exact_reviewed_current_user_code_identity"
        );
        let mut bad_digest = base.clone();
        set(&mut bad_digest, &|_, d, _, _, _| {
            *d = Sha256Digest("Z".into())
        });
        assert_eq!(
            reason(admit(&bad_digest, &workspace)),
            "stdio_requires_exact_reviewed_current_user_code_identity"
        );
        let mut changed = base.clone();
        set(&mut changed, &|_, d, _, _, _| {
            *d = Sha256Digest("0".repeat(64))
        });
        assert_eq!(
            reason(admit(&changed, &workspace)),
            "executable_identity_changed"
        );
        let mut args = base.clone();
        set(&mut args, &|_, _, a, _, _| *a = vec!["--other".into()]);
        assert_eq!(
            reason(admit(&args, &workspace)),
            "reviewed_command_or_arguments_do_not_match_pinned_manifest"
        );
        let mut server = base.clone();
        server.namespace = "sample.absent".into();
        assert_eq!(
            reason(admit(&server, &workspace)),
            "server_not_in_portable_manifest"
        );
        let mut http_server = base.clone();
        http_server.namespace = "sample.web".into();
        assert_eq!(
            reason(admit(&http_server, &workspace)),
            "transport_declaration_mismatch"
        );
        let mut outside = base.clone();
        set(&mut outside, &|_, _, _, c, _| {
            *c = root.path().to_string_lossy().into_owned()
        });
        assert_eq!(
            reason(admit(&outside, &workspace)),
            "working_directory_not_in_reviewed_package_or_workspace"
        );
        let mut gone = base.clone();
        set(&mut gone, &|_, _, _, c, _| {
            *c = root.path().join("gone").to_string_lossy().into_owned()
        });
        assert_eq!(
            reason(admit(&gone, &workspace)),
            "working_directory_unavailable"
        );
        let mut input = base.clone();
        set(&mut input, &|_, _, _, _, i| {
            i.insert("relative.txt".into(), Sha256Digest("0".repeat(64)));
        });
        assert_eq!(
            reason(admit(&input, &workspace)),
            "invalid_dependency_identity"
        );
        let file = workspace.join("input.txt");
        std::fs::write(&file, "input").unwrap();
        let mut stale = base.clone();
        set(&mut stale, &|_, _, _, _, i| {
            i.insert(
                file.to_string_lossy().into_owned(),
                Sha256Digest("0".repeat(64)),
            );
        });
        assert_eq!(
            reason(admit(&stale, &workspace)),
            "dependency_identity_or_capacity_changed"
        );
        let mut absent = base.clone();
        set(&mut absent, &|_, _, _, _, i| {
            i.insert(
                workspace.join("absent.txt").to_string_lossy().into_owned(),
                Sha256Digest("0".repeat(64)),
            );
        });
        assert_eq!(
            reason(admit(&absent, &workspace)),
            "dependency_file_unavailable"
        );
        let mut in_package = base.clone();
        set(&mut in_package, &|_, _, _, c, _| {
            *c = package.to_string_lossy().into_owned()
        });
        assert!(admit(&in_package, &workspace).is_ok());
    }

    #[test]
    fn http_servers_need_a_reviewed_remote_boundary_and_matching_endpoint() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let (package, digest) = package(root.path(), "sample", false);
        let mut definition = stdio_definition(&package, &digest, &workspace);
        definition.namespace = "sample.web".into();
        definition.trust = ExtensionTrust::ReviewedRemoteService;
        let transport = |endpoint: &str, address: &str, account: Option<&str>| {
            ExtensionTransport::StreamableHttp {
                endpoint: endpoint.into(),
                addresses: vec![address.into()],
                local_demo: true,
                credential_account: account.map(Into::into),
            }
        };
        definition.transport = transport("http://127.0.0.1:18081/mcp", "127.0.0.1:18081", None);
        let admitted = admit(&definition, &workspace).unwrap();
        assert!(admitted.program_spec.is_none());
        let mut trust = definition.clone();
        trust.trust = ExtensionTrust::ReviewedCurrentUserCode;
        assert_eq!(
            reason(admit(&trust, &workspace)),
            "HTTP_requires_reviewed_remote_service_boundary"
        );
        let mut endpoint = definition.clone();
        endpoint.transport = transport("http://127.0.0.1:18082/mcp", "127.0.0.1:18082", None);
        assert_eq!(
            reason(admit(&endpoint, &workspace)),
            "endpoint_does_not_match_portable_declaration"
        );
        let mut address = definition.clone();
        address.transport = transport("http://127.0.0.1:18081/mcp", "not an address", None);
        assert_eq!(
            reason(admit(&address, &workspace)),
            "invalid_reviewed_addresses"
        );
        let mut account = definition.clone();
        account.transport = transport("http://127.0.0.1:18081/mcp", "127.0.0.1:18081", Some(""));
        assert_eq!(
            reason(admit(&account, &workspace)),
            "invalid_secure_account_binding"
        );
        let mut port = definition.clone();
        port.transport = transport("http://127.0.0.1:18081/mcp", "127.0.0.1:9", None);
        assert_eq!(
            reason(admit(&port, &workspace)),
            "HTTP_destination_review_failed"
        );
        let mut stdio_server = definition.clone();
        stdio_server.namespace = "sample.stdio".into();
        assert_eq!(
            reason(admit(&stdio_server, &workspace)),
            "endpoint_does_not_match_portable_declaration"
        );
    }

    #[test]
    fn skills_are_read_only_instructions_without_tools_or_network() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let (package, digest) = package(root.path(), "notes", true);
        let mut definition = stdio_definition(&package, &digest, &workspace);
        definition.namespace = "notes.skills".into();
        definition.transport = ExtensionTransport::Skills;
        definition.trust = ExtensionTrust::ReadOnlyInstructions;
        definition.allowed_tools.clear();
        assert!(admit(&definition, &workspace)
            .unwrap()
            .program_spec
            .is_none());
        let mut tools = definition.clone();
        tools.allowed_tools = vec!["read".into()];
        assert_eq!(
            reason(admit(&tools, &workspace)),
            "skills_only_configuration_must_not_request_execution_or_network"
        );
        let mut trust = definition.clone();
        trust.trust = ExtensionTrust::ReviewedCurrentUserCode;
        assert_eq!(
            reason(admit(&trust, &workspace)),
            "skills_only_configuration_must_not_request_execution_or_network"
        );
        let (bare, bare_digest) = super::tests::package(root.path(), "bare", false);
        let mut no_skills = definition.clone();
        no_skills.namespace = "bare.skills".into();
        no_skills.package_root = bare.to_string_lossy().into_owned();
        no_skills.package_sha256 = Sha256Digest(bare_digest);
        assert_eq!(
            reason(admit(&no_skills, &workspace)),
            "skills_only_configuration_must_not_request_execution_or_network"
        );
    }

    #[test]
    fn image_scope_needs_an_absolute_private_folder_exact_origins_and_trusted_code() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let (package, digest) = package(root.path(), "sample", false);
        let base = stdio_definition(&package, &digest, &workspace);
        let scope = |directory: &str, origins: &[&str]| {
            Some(ExtensionImageScope {
                directory: directory.into(),
                origins: origins.iter().map(|o| (*o).to_owned()).collect(),
            })
        };
        let media = root.path().join("media");
        std::fs::create_dir(&media).unwrap();
        let media_path = media.to_string_lossy().into_owned();
        let mut relative = base.clone();
        relative.image_scope = scope("media", &["http://127.0.0.1:18080"]);
        assert_eq!(
            reason(admit(&relative, &workspace)),
            "image_scope_requires_explicit_owned_directory_and_trusted_producer"
        );
        let mut empty = base.clone();
        empty.image_scope = scope(&media_path, &[]);
        assert_eq!(
            reason(admit(&empty, &workspace)),
            "image_scope_requires_explicit_owned_directory_and_trusted_producer"
        );
        let mut too_many = base.clone();
        too_many.image_scope = scope(&media_path, &["http://127.0.0.1:1"; 17]);
        assert_eq!(
            reason(admit(&too_many, &workspace)),
            "image_scope_requires_explicit_owned_directory_and_trusted_producer"
        );
        let mut untrusted = base.clone();
        untrusted.trust = ExtensionTrust::ReviewedRemoteService;
        untrusted.image_scope = scope(&media_path, &["http://127.0.0.1:18080"]);
        assert_eq!(
            reason(admit(&untrusted, &workspace)),
            "image_scope_requires_explicit_owned_directory_and_trusted_producer"
        );
        let mut not_exact = base.clone();
        not_exact.image_scope = scope(&media_path, &["http://127.0.0.1:18080/path"]);
        assert_eq!(
            reason(admit(&not_exact, &workspace)),
            "image_scope_must_use_exact_origins"
        );
        let mut invalid = base.clone();
        invalid.image_scope = scope(&media_path, &["not a url"]);
        assert_eq!(reason(admit(&invalid, &workspace)), "invalid_image_origin");
    }
}
