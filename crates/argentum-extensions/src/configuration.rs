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
