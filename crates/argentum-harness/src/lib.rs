use std::collections::{BTreeMap, BTreeSet};

use argentum_domain::{
    ApprovalMode, HarnessAvailability, HarnessCapabilityKind, HarnessCapabilityState,
    HarnessExecutionPolicy, HarnessExecutionProfileSummary, HarnessProfileSummary,
    HarnessReadiness, HarnessSnapshot, HarnessSurfaceState, LayoutProfile, SandboxScope, SurfaceId,
    CUSTOM_EXECUTION_PROFILE_ID, DEFAULT_EXECUTION_PROFILE_ID, LEGACY_GUIDED_EXECUTION_PROFILE_ID,
    LEGACY_READ_ONLY_EXECUTION_PROFILE_ID,
};
use thiserror::Error;

mod first_party;
mod lifecycle;
mod service_host;
mod services;

pub use service_host::{ServiceHost, ServiceHostError, ServiceModuleStatus, ServiceRunLease};

pub use services::{
    ResolvedComposition, ServiceCatalog, ServiceCompositionError, ServiceContract, ServiceModule,
    ServiceRequirement,
};

pub use argentum_lifecycle::{EffectDisposer, EffectFailure, ModuleState};
pub use argentum_lifecycle::{LifecycleSnapshot, MountContext, MountFuture};
pub use first_party::{
    first_party_review_diff_module, FIRST_PARTY_REVIEW_DIFF_MODULE_ID, REVIEW_DIFF_CAPABILITY_ID,
};
pub use lifecycle::{
    ComposableHarness, ComposableHarnessError, HarnessModuleHandle, HarnessModuleRegistration,
};

pub const DEFAULT_PROFILE_ID: &str = "standard";
pub const CUSTOM_PROFILE_ID: &str = "custom";

const MAX_REGISTRATIONS: usize = 128;
const MAX_ID_BYTES: usize = 64;
const MAX_LABEL_BYTES: usize = 80;
const MAX_DETAIL_BYTES: usize = 512;
const MAX_DEPENDENCIES: usize = 16;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HarnessError {
    #[error("harness ID must use 1 to 64 lowercase ASCII letters, digits, dots, or hyphens")]
    InvalidId,
    #[error("harness label must use 1 to 80 printable characters")]
    InvalidLabel,
    #[error("harness detail must not exceed 512 bytes")]
    InvalidDetail,
    #[error("harness registration limit reached")]
    RegistrationLimit,
    #[error("harness registration '{0}' already exists")]
    DuplicateRegistration(String),
    #[error("harness capability '{capability}' depends on unknown capability '{dependency}'")]
    MissingDependency {
        capability: String,
        dependency: String,
    },
    #[error("harness capability dependency cycle includes '{0}'")]
    DependencyCycle(String),
    #[error("harness profile '{0}' is not selectable")]
    UnknownProfile(String),
    #[error("surface '{0}' is not registered")]
    UnknownSurface(&'static str),
    #[error("surface '{0}' is not available in this build")]
    UnavailableSurface(&'static str),
    #[error("surface '{0}' has lifecycle-controlled visibility")]
    FixedSurface(&'static str),
    #[error("surface '{0}' is not an inspector surface")]
    InvalidInspectorSurface(&'static str),
    #[error("inspector surface '{0}' is not enabled")]
    InspectorSurfaceDisabled(&'static str),
    #[error("profile '{profile}' tries to show unavailable surface '{surface}'")]
    InvalidProfileSurface {
        profile: String,
        surface: &'static str,
    },
    #[error("harness capability dependencies are limited to 16 entries")]
    DependencyLimit,
    #[error("harness capability '{0}' is not registered")]
    UnknownCapability(String),
    #[error("harness capability '{0}' is not available in this build")]
    UnavailableCapability(String),
    #[error("harness capability '{0}' has fixed enablement")]
    FixedCapability(String),
    #[error("harness capability '{capability}' requires enabled capability '{dependency}'")]
    CapabilityDependencyRequired {
        capability: String,
        dependency: String,
    },
    #[error("execution profile '{0}' is not selectable")]
    UnknownExecutionProfile(String),
    #[error("execution profile '{profile}' references invalid capability '{capability}'")]
    InvalidExecutionProfileCapability { profile: String, capability: String },
}

#[derive(Debug, Clone, Default)]
pub struct HarnessFacts {
    readiness: BTreeMap<String, HarnessReadiness>,
}

impl HarnessFacts {
    pub fn with_readiness(
        mut self,
        capability_id: impl Into<String>,
        readiness: HarnessReadiness,
    ) -> Self {
        self.readiness.insert(capability_id.into(), readiness);
        self
    }
}

#[derive(Debug, Clone)]
pub struct CapabilityRegistration {
    pub id: String,
    pub label: String,
    pub kind: HarnessCapabilityKind,
    pub available: bool,
    pub enabled: bool,
    pub configurable: bool,
    pub readiness: HarnessReadiness,
    pub detail: String,
    pub unavailable_reason: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SurfaceRegistration {
    pub id: SurfaceId,
    pub available: bool,
    pub configurable: bool,
    pub detail: String,
    pub unavailable_reason: String,
}

#[derive(Debug, Clone)]
pub struct ProfileRegistration {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub order: u16,
    pub visibility: BTreeMap<SurfaceId, bool>,
}

#[derive(Debug, Clone)]
pub struct ExecutionProfileRegistration {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub order: u16,
    pub sandbox_scope: SandboxScope,
    pub approval_mode: ApprovalMode,
    pub capability_enabled: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Default)]
pub struct HarnessRegistry {
    capabilities: BTreeMap<String, CapabilityRegistration>,
    surfaces: BTreeMap<SurfaceId, SurfaceRegistration>,
    profiles: BTreeMap<String, ProfileRegistration>,
    execution_profiles: BTreeMap<String, ExecutionProfileRegistration>,
}

impl HarnessRegistry {
    pub fn built_in() -> Result<Self, HarnessError> {
        let mut registry = Self::default();
        for capability in built_in_capabilities() {
            registry.register_capability(capability)?;
        }
        for surface in built_in_surfaces() {
            registry.register_surface(surface)?;
        }
        for profile in built_in_profiles() {
            registry.register_profile(profile)?;
        }
        for profile in built_in_execution_profiles() {
            registry.register_execution_profile(profile)?;
        }
        registry.validate()?;
        Ok(registry)
    }

    pub fn register_capability(
        &mut self,
        registration: CapabilityRegistration,
    ) -> Result<(), HarnessError> {
        validate_id(&registration.id)?;
        validate_label(&registration.label)?;
        validate_detail(&registration.detail)?;
        validate_detail(&registration.unavailable_reason)?;
        if registration.dependencies.len() > MAX_DEPENDENCIES {
            return Err(HarnessError::DependencyLimit);
        }
        for dependency in &registration.dependencies {
            validate_id(dependency)?;
        }
        if self.capabilities.len() >= MAX_REGISTRATIONS {
            return Err(HarnessError::RegistrationLimit);
        }
        let id = registration.id.clone();
        if self.capabilities.contains_key(&id) {
            return Err(HarnessError::DuplicateRegistration(id));
        }
        self.capabilities.insert(id, registration);
        Ok(())
    }

    pub fn register_surface(
        &mut self,
        registration: SurfaceRegistration,
    ) -> Result<(), HarnessError> {
        validate_detail(&registration.detail)?;
        validate_detail(&registration.unavailable_reason)?;
        if self.surfaces.len() >= MAX_REGISTRATIONS {
            return Err(HarnessError::RegistrationLimit);
        }
        let id = registration.id;
        if self.surfaces.contains_key(&id) {
            return Err(HarnessError::DuplicateRegistration(
                id.label().to_ascii_lowercase(),
            ));
        }
        self.surfaces.insert(id, registration);
        Ok(())
    }

    pub fn register_profile(
        &mut self,
        registration: ProfileRegistration,
    ) -> Result<(), HarnessError> {
        validate_id(&registration.id)?;
        validate_label(&registration.label)?;
        validate_detail(&registration.detail)?;
        if self.profiles.len() >= MAX_REGISTRATIONS {
            return Err(HarnessError::RegistrationLimit);
        }
        for (surface, visible) in &registration.visibility {
            let Some(surface_registration) = self.surfaces.get(surface) else {
                return Err(HarnessError::UnknownSurface(surface.label()));
            };
            if *visible && !surface_registration.available {
                return Err(HarnessError::InvalidProfileSurface {
                    profile: registration.id.clone(),
                    surface: surface.label(),
                });
            }
        }
        let id = registration.id.clone();
        if self.profiles.contains_key(&id) {
            return Err(HarnessError::DuplicateRegistration(id));
        }
        self.profiles.insert(id, registration);
        Ok(())
    }

    pub fn register_execution_profile(
        &mut self,
        registration: ExecutionProfileRegistration,
    ) -> Result<(), HarnessError> {
        validate_id(&registration.id)?;
        validate_label(&registration.label)?;
        validate_detail(&registration.detail)?;
        if self.execution_profiles.len() >= MAX_REGISTRATIONS {
            return Err(HarnessError::RegistrationLimit);
        }
        for capability_id in registration.capability_enabled.keys() {
            let Some(capability) = self.capabilities.get(capability_id) else {
                return Err(HarnessError::InvalidExecutionProfileCapability {
                    profile: registration.id.clone(),
                    capability: capability_id.clone(),
                });
            };
            if !capability.available || !capability.configurable {
                return Err(HarnessError::InvalidExecutionProfileCapability {
                    profile: registration.id.clone(),
                    capability: capability_id.clone(),
                });
            }
        }
        let id = registration.id.clone();
        if self.execution_profiles.contains_key(&id) {
            return Err(HarnessError::DuplicateRegistration(id));
        }
        self.execution_profiles.insert(id, registration);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), HarnessError> {
        for (id, capability) in &self.capabilities {
            for dependency in &capability.dependencies {
                if !self.capabilities.contains_key(dependency) {
                    return Err(HarnessError::MissingDependency {
                        capability: id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }

        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        for id in self.capabilities.keys() {
            self.visit_capability(id, &mut visiting, &mut visited)?;
        }
        Ok(())
    }

    pub fn snapshot(
        &self,
        layout: &LayoutProfile,
        execution_policy: &HarnessExecutionPolicy,
        facts: &HarnessFacts,
    ) -> HarnessSnapshot {
        let execution_policy = self.reconcile_execution_policy(execution_policy);
        let selected_profile_id = if self.profiles.contains_key(&layout.harness_profile_id) {
            layout.harness_profile_id.clone()
        } else {
            CUSTOM_PROFILE_ID.to_owned()
        };

        let mut profiles = self
            .profiles
            .values()
            .map(|profile| HarnessProfileSummary {
                id: profile.id.clone(),
                label: profile.label.clone(),
                detail: profile.detail.clone(),
                selected: profile.id == selected_profile_id,
                selectable: true,
            })
            .collect::<Vec<_>>();
        profiles.sort_by_key(|profile| {
            self.profiles
                .get(&profile.id)
                .map_or(u16::MAX, |definition| definition.order)
        });
        profiles.push(HarnessProfileSummary {
            id: CUSTOM_PROFILE_ID.to_owned(),
            label: "Custom".to_owned(),
            detail: "Individual surface visibility saved for this workspace.".to_owned(),
            selected: selected_profile_id == CUSTOM_PROFILE_ID,
            selectable: false,
        });

        let selected_execution_profile_id = if self
            .execution_profiles
            .contains_key(&execution_policy.profile_id)
        {
            execution_policy.profile_id.clone()
        } else {
            CUSTOM_EXECUTION_PROFILE_ID.to_owned()
        };
        let mut execution_profiles = self
            .execution_profiles
            .values()
            .map(|profile| HarnessExecutionProfileSummary {
                id: profile.id.clone(),
                label: profile.label.clone(),
                detail: profile.detail.clone(),
                selected: profile.id == selected_execution_profile_id,
                selectable: true,
            })
            .collect::<Vec<_>>();
        execution_profiles.sort_by_key(|profile| {
            self.execution_profiles
                .get(&profile.id)
                .map_or(u16::MAX, |definition| definition.order)
        });
        execution_profiles.push(HarnessExecutionProfileSummary {
            id: CUSTOM_EXECUTION_PROFILE_ID.to_owned(),
            label: "Custom".to_owned(),
            detail: "Exact developer-tool enablement saved for this project.".to_owned(),
            selected: selected_execution_profile_id == CUSTOM_EXECUTION_PROFILE_ID,
            selectable: false,
        });

        let capabilities = self
            .capabilities
            .values()
            .map(|capability| self.capability_state(capability, &execution_policy, facts))
            .collect();
        let surfaces = self
            .surfaces
            .values()
            .map(|surface| HarnessSurfaceState {
                id: surface.id,
                label: surface.id.label().to_owned(),
                availability: availability(surface.available),
                visible: surface.available
                    && layout.visible.get(&surface.id).copied().unwrap_or(false),
                configurable: surface.available && surface.configurable,
                detail: surface.detail.clone(),
                unavailable_reason: if surface.available {
                    String::new()
                } else {
                    surface.unavailable_reason.clone()
                },
            })
            .collect();

        HarnessSnapshot {
            diagnostics: None,
            local_backend: None,
            modules: Vec::new(),
            selected_profile_id,
            profiles,
            selected_execution_profile_id,
            execution_profiles,
            capabilities,
            surfaces,
        }
    }

    pub fn apply_profile(
        &self,
        layout: &LayoutProfile,
        profile_id: &str,
    ) -> Result<LayoutProfile, HarnessError> {
        let profile_id = profile_id.trim();
        let Some(profile) = self.profiles.get(profile_id) else {
            return Err(HarnessError::UnknownProfile(profile_id.to_owned()));
        };
        let mut resolved = layout.clone();
        resolved.harness_profile_id = profile.id.clone();
        for surface in self.surfaces.values() {
            if !surface.available {
                resolved.visible.insert(surface.id, false);
            } else if surface.configurable {
                resolved.visible.insert(
                    surface.id,
                    profile
                        .visibility
                        .get(&surface.id)
                        .copied()
                        .unwrap_or(false),
                );
            }
        }
        resolved.active_inspector = self.first_enabled_inspector(&resolved);
        resolved.inspector_collapsed =
            matches!(profile.id.as_str(), "focused" | DEFAULT_PROFILE_ID)
                || resolved.active_inspector.is_none();
        Ok(resolved)
    }

    fn first_enabled_inspector(&self, layout: &LayoutProfile) -> Option<SurfaceId> {
        [
            SurfaceId::Changes,
            SurfaceId::Activity,
            SurfaceId::Trajectory,
        ]
        .into_iter()
        .find(|surface| {
            self.surfaces
                .get(surface)
                .is_some_and(|registration| registration.available)
                && layout.visible.get(surface).copied().unwrap_or(false)
        })
    }

    pub fn set_surface_visibility(
        &self,
        layout: &LayoutProfile,
        surface: SurfaceId,
        visible: bool,
    ) -> Result<LayoutProfile, HarnessError> {
        let Some(registration) = self.surfaces.get(&surface) else {
            return Err(HarnessError::UnknownSurface(surface.label()));
        };
        if !registration.available {
            return Err(HarnessError::UnavailableSurface(surface.label()));
        }
        if !registration.configurable {
            return Err(HarnessError::FixedSurface(surface.label()));
        }
        let mut resolved = layout.clone();
        resolved.harness_profile_id = CUSTOM_PROFILE_ID.to_owned();
        resolved.visible.insert(surface, visible);
        Ok(self.reconcile_layout(&resolved))
    }

    pub fn set_inspector_surface(
        &self,
        layout: &LayoutProfile,
        surface: Option<SurfaceId>,
    ) -> Result<LayoutProfile, HarnessError> {
        let Some(surface) = surface else {
            let mut resolved = layout.clone();
            resolved.inspector_collapsed = true;
            return Ok(resolved);
        };
        if !surface.is_inspector() {
            return Err(HarnessError::InvalidInspectorSurface(surface.label()));
        }
        let Some(registration) = self.surfaces.get(&surface) else {
            return Err(HarnessError::UnknownSurface(surface.label()));
        };
        if !registration.available {
            return Err(HarnessError::UnavailableSurface(surface.label()));
        }
        if !layout.visible.get(&surface).copied().unwrap_or(false) {
            return Err(HarnessError::InspectorSurfaceDisabled(surface.label()));
        }

        let mut resolved = layout.clone();
        resolved.active_inspector = Some(surface);
        resolved.inspector_collapsed = false;
        Ok(resolved)
    }

    pub fn reconcile_layout(&self, layout: &LayoutProfile) -> LayoutProfile {
        let mut resolved = layout.clone();
        for surface in self.surfaces.values() {
            if !surface.available {
                resolved.visible.insert(surface.id, false);
            } else if !surface.configurable {
                resolved.visible.insert(
                    surface.id,
                    matches!(surface.id, SurfaceId::Conversation | SurfaceId::Plan),
                );
            }
        }
        if let Some(profile) = self.profiles.get(&resolved.harness_profile_id) {
            let matches_profile = self.surfaces.values().all(|surface| {
                !surface.configurable
                    || resolved.visible.get(&surface.id).copied().unwrap_or(false)
                        == profile
                            .visibility
                            .get(&surface.id)
                            .copied()
                            .unwrap_or(false)
            });
            if !matches_profile {
                resolved.harness_profile_id = CUSTOM_PROFILE_ID.to_owned();
            }
        } else {
            resolved.harness_profile_id = CUSTOM_PROFILE_ID.to_owned();
        }
        let active_is_eligible = resolved.active_inspector.is_some_and(|surface| {
            surface.is_inspector()
                && self
                    .surfaces
                    .get(&surface)
                    .is_some_and(|registration| registration.available)
                && resolved.visible.get(&surface).copied().unwrap_or(false)
        });
        if !active_is_eligible {
            resolved.active_inspector = self.first_enabled_inspector(&resolved);
        }
        if resolved.active_inspector.is_none() {
            resolved.inspector_collapsed = true;
        }
        resolved
    }

    pub fn apply_execution_profile(
        &self,
        policy: &HarnessExecutionPolicy,
        profile_id: &str,
    ) -> Result<HarnessExecutionPolicy, HarnessError> {
        let profile_id = match profile_id.trim() {
            LEGACY_READ_ONLY_EXECUTION_PROFILE_ID => "observe",
            LEGACY_GUIDED_EXECUTION_PROFILE_ID => DEFAULT_EXECUTION_PROFILE_ID,
            profile_id => profile_id,
        };
        let Some(profile) = self.execution_profiles.get(profile_id) else {
            return Err(HarnessError::UnknownExecutionProfile(profile_id.to_owned()));
        };
        let mut resolved = policy.clone();
        resolved.profile_id = profile.id.clone();
        resolved.source_profile_id = Some(profile.id.clone());
        resolved.sandbox_scope = profile.sandbox_scope;
        resolved.approval_mode = profile.approval_mode;
        for capability in self.capabilities.values().filter(|item| item.configurable) {
            resolved.capability_enabled.insert(
                capability.id.clone(),
                profile
                    .capability_enabled
                    .get(&capability.id)
                    .copied()
                    .unwrap_or(false),
            );
        }
        Ok(self.reconcile_execution_policy(&resolved))
    }

    pub fn set_capability_enabled(
        &self,
        policy: &HarnessExecutionPolicy,
        capability_id: &str,
        enabled: bool,
    ) -> Result<HarnessExecutionPolicy, HarnessError> {
        let capability_id = capability_id.trim();
        let Some(capability) = self.capabilities.get(capability_id) else {
            return Err(HarnessError::UnknownCapability(capability_id.to_owned()));
        };
        if !capability.available {
            return Err(HarnessError::UnavailableCapability(
                capability_id.to_owned(),
            ));
        }
        if !capability.configurable {
            return Err(HarnessError::FixedCapability(capability_id.to_owned()));
        }

        let mut resolved = self.reconcile_execution_policy(policy);
        if resolved.source_profile_id.is_none()
            && resolved.profile_id != CUSTOM_EXECUTION_PROFILE_ID
        {
            resolved.source_profile_id = Some(resolved.profile_id.clone());
        }
        resolved.profile_id = CUSTOM_EXECUTION_PROFILE_ID.to_owned();
        if resolved.sandbox_scope == SandboxScope::Unrestricted {
            resolved.sandbox_scope = SandboxScope::WorkspaceWrite;
            resolved.approval_mode = ApprovalMode::OnRisk;
        }
        resolved
            .capability_enabled
            .insert(capability_id.to_owned(), enabled);

        if enabled {
            for dependency in &capability.dependencies {
                if !self.capability_is_enabled(&resolved, dependency) {
                    return Err(HarnessError::CapabilityDependencyRequired {
                        capability: capability_id.to_owned(),
                        dependency: dependency.clone(),
                    });
                }
            }
        } else if let Some(dependent) = self.capabilities.values().find(|candidate| {
            candidate
                .dependencies
                .iter()
                .any(|item| item == capability_id)
                && self.capability_is_enabled(&resolved, &candidate.id)
        }) {
            return Err(HarnessError::CapabilityDependencyRequired {
                capability: dependent.id.clone(),
                dependency: capability_id.to_owned(),
            });
        }
        Ok(resolved)
    }

    pub fn reconcile_execution_policy(
        &self,
        policy: &HarnessExecutionPolicy,
    ) -> HarnessExecutionPolicy {
        if matches!(
            policy.profile_id.as_str(),
            LEGACY_READ_ONLY_EXECUTION_PROFILE_ID | LEGACY_GUIDED_EXECUTION_PROFILE_ID
        ) {
            let profile_id = if policy.profile_id == LEGACY_READ_ONLY_EXECUTION_PROFILE_ID {
                "observe"
            } else {
                DEFAULT_EXECUTION_PROFILE_ID
            };
            let profile = &self.execution_profiles[profile_id];
            return HarnessExecutionPolicy {
                profile_id: profile.id.clone(),
                source_profile_id: Some(profile.id.clone()),
                sandbox_scope: profile.sandbox_scope,
                approval_mode: profile.approval_mode,
                capability_enabled: profile.capability_enabled.clone(),
            };
        }

        let mut resolved = policy.clone();
        if resolved.sandbox_scope == SandboxScope::Unrestricted
            && resolved.profile_id != "full-access"
        {
            resolved.sandbox_scope = SandboxScope::WorkspaceWrite;
            resolved.approval_mode = ApprovalMode::OnRisk;
        }
        resolved.capability_enabled.retain(|id, _| {
            self.capabilities
                .get(id)
                .is_some_and(|capability| capability.available && capability.configurable)
        });
        for capability in self.capabilities.values().filter(|item| item.configurable) {
            resolved
                .capability_enabled
                .entry(capability.id.clone())
                .or_insert(capability.enabled);
        }

        loop {
            let blocked = self
                .capabilities
                .values()
                .filter(|capability| capability.configurable)
                .filter(|capability| self.capability_is_enabled(&resolved, &capability.id))
                .find(|capability| {
                    capability
                        .dependencies
                        .iter()
                        .any(|dependency| !self.capability_is_enabled(&resolved, dependency))
                })
                .map(|capability| capability.id.clone());
            let Some(blocked) = blocked else {
                break;
            };
            resolved.capability_enabled.insert(blocked, false);
        }

        if let Some(profile) = self.execution_profiles.get(&resolved.profile_id) {
            let matches_profile = resolved.sandbox_scope == profile.sandbox_scope
                && resolved.approval_mode == profile.approval_mode
                && self
                    .capabilities
                    .values()
                    .filter(|capability| capability.configurable)
                    .all(|capability| {
                        self.capability_is_enabled(&resolved, &capability.id)
                            == profile
                                .capability_enabled
                                .get(&capability.id)
                                .copied()
                                .unwrap_or(false)
                    });
            if !matches_profile {
                resolved.profile_id = CUSTOM_EXECUTION_PROFILE_ID.to_owned();
            }
        } else {
            resolved.profile_id = CUSTOM_EXECUTION_PROFILE_ID.to_owned();
        }
        resolved
    }

    pub fn capability_is_enabled(
        &self,
        policy: &HarnessExecutionPolicy,
        capability_id: &str,
    ) -> bool {
        let Some(capability) = self.capabilities.get(capability_id) else {
            return false;
        };
        capability.available
            && if capability.configurable {
                policy
                    .capability_enabled
                    .get(capability_id)
                    .copied()
                    .unwrap_or(capability.enabled)
            } else {
                capability.enabled
            }
    }

    fn capability_state(
        &self,
        capability: &CapabilityRegistration,
        execution_policy: &HarnessExecutionPolicy,
        facts: &HarnessFacts,
    ) -> HarnessCapabilityState {
        let enabled = self.capability_is_enabled(execution_policy, &capability.id);
        let mut readiness = if capability.available {
            facts
                .readiness
                .get(&capability.id)
                .copied()
                .unwrap_or(capability.readiness)
        } else {
            HarnessReadiness::Unavailable
        };
        if enabled {
            for dependency in &capability.dependencies {
                let Some(dependency) = self.capabilities.get(dependency) else {
                    readiness = HarnessReadiness::Blocked;
                    break;
                };
                if !dependency.available
                    || !self.capability_is_enabled(execution_policy, &dependency.id)
                {
                    readiness = HarnessReadiness::Blocked;
                    break;
                }
            }
        }
        HarnessCapabilityState {
            id: capability.id.clone(),
            label: capability.label.clone(),
            kind: capability.kind,
            availability: availability(capability.available),
            readiness,
            enabled,
            configurable: capability.available && capability.configurable,
            detail: capability.detail.clone(),
            unavailable_reason: if capability.available {
                String::new()
            } else {
                capability.unavailable_reason.clone()
            },
            dependencies: capability.dependencies.clone(),
        }
    }

    fn visit_capability(
        &self,
        id: &str,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> Result<(), HarnessError> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_owned()) {
            return Err(HarnessError::DependencyCycle(id.to_owned()));
        }
        if let Some(capability) = self.capabilities.get(id) {
            for dependency in &capability.dependencies {
                self.visit_capability(dependency, visiting, visited)?;
            }
        }
        visiting.remove(id);
        visited.insert(id.to_owned());
        Ok(())
    }
}

fn availability(available: bool) -> HarnessAvailability {
    if available {
        HarnessAvailability::Available
    } else {
        HarnessAvailability::Unavailable
    }
}

fn validate_id(id: &str) -> Result<(), HarnessError> {
    if id.is_empty()
        || id.len() > MAX_ID_BYTES
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
    {
        return Err(HarnessError::InvalidId);
    }
    Ok(())
}

fn validate_label(label: &str) -> Result<(), HarnessError> {
    if label.trim().is_empty()
        || label.len() > MAX_LABEL_BYTES
        || label.chars().any(char::is_control)
    {
        return Err(HarnessError::InvalidLabel);
    }
    Ok(())
}

fn validate_detail(detail: &str) -> Result<(), HarnessError> {
    if detail.len() > MAX_DETAIL_BYTES || detail.chars().any(char::is_control) {
        return Err(HarnessError::InvalidDetail);
    }
    Ok(())
}

fn built_in_capabilities() -> Vec<CapabilityRegistration> {
    vec![
        available_capability(
            "approval.write",
            "Execution authorization",
            HarnessCapabilityKind::Security,
            "Execution decisions are bound to the selected profile, persisted grants, and trusted local authority.",
            vec![],
        ),
        available_capability(
            "changes.accounting",
            "Changed-file accounting",
            HarnessCapabilityKind::Review,
            "Durable per-file records and bounded snapshots are committed for successful workspace writes.",
            vec!["approval.write"],
        ),
        configurable_capability(
            "execution.argv",
            "Argument-vector process execution",
            HarnessCapabilityKind::Execution,
            "Bounded child processes run from an explicit program and argument vector.",
            vec!["approval.write"],
        ),
        configurable_capability(
            "execution.shell",
            "Shell execution",
            HarnessCapabilityKind::Execution,
            "Bounded shell commands are distinct from argument-vector execution and carry higher risk.",
            vec!["approval.write"],
        ),
        unavailable_capability(
            "execution.terminal",
            "Persistent terminal",
            HarnessCapabilityKind::Execution,
            "No PTY or persistent terminal backend is registered.",
        ),
        external_extension_capability(),
        available_capability(
            "goal.session",
            "Session goals",
            HarnessCapabilityKind::Agent,
            "One optional durable goal contract can be attached to a session.",
            vec!["session.history"],
        ),
        unavailable_capability(
            "integration.browser",
            "Browser control",
            HarnessCapabilityKind::Integration,
            "No direct browser registration. Extensions-enabled builds support the separately owner-reviewed first-party local Playwright adapter through extension.external; inspect its active definition and dependencies.",
        ),
        available_capability(
            "model.reasoning",
            "Provider reasoning",
            HarnessCapabilityKind::Provider,
            "Provider-reported reasoning is separated from visible answer text.",
            vec!["model.streaming"],
        ),
        available_capability(
            "model.streaming",
            "Streamed model turns",
            HarnessCapabilityKind::Provider,
            "The selected exact provider profile streams through CommandHost.",
            vec!["provider.profiles"],
        ),
        disabled_configurable_capability(
            "model.delegate-readonly",
            "Read-only delegated investigation",
            HarnessCapabilityKind::Agent,
            "Allow one bounded sequential worker investigation on the selected compatible-HTTP coding route. The worker can only use enabled read_text, search_text, and search_filenames capabilities.",
            vec!["model.streaming", "tool.read-text"],
        ),
        available_capability(
            "model.usage",
            "Reported model usage",
            HarnessCapabilityKind::Provider,
            "Token usage and context limits are shown only when reported or verified.",
            vec!["model.streaming"],
        ),
        available_capability(
            "provider.catalogs",
            "Provider model catalogs",
            HarnessCapabilityKind::Provider,
            "Bounded model lists are resolved for one exact provider profile.",
            vec!["provider.profiles"],
        ),
        available_capability(
            "provider.profiles",
            "Provider profiles",
            HarnessCapabilityKind::Provider,
            "Workspace-scoped provider profiles and exact model selection are durable.",
            vec![],
        ),
        first_party::review_diff_capability(),
        available_capability(
            "session.history",
            "Durable session history",
            HarnessCapabilityKind::Session,
            "Ordered user and assistant messages restore across process restarts.",
            vec![],
        ),
        configurable_capability("tool.list-directory", "List directory", HarnessCapabilityKind::Tool, "Bounded directory listings are available.", vec![]),
        configurable_capability("tool.file-metadata", "File metadata", HarnessCapabilityKind::Tool, "Bounded file metadata inspection is available.", vec![]),
        configurable_capability("tool.read-text", "Read text", HarnessCapabilityKind::Tool, "Bounded UTF-8 text reads are available.", vec![]),
        configurable_capability("tool.read-binary-metadata", "Binary metadata", HarnessCapabilityKind::Tool, "Binary files can be inspected without returning raw binary content.", vec![]),
        configurable_capability("tool.search-text", "Search text", HarnessCapabilityKind::Tool, "Bounded workspace text search is available.", vec![]),
        configurable_capability("tool.search-filenames", "Search filenames", HarnessCapabilityKind::Tool, "Bounded workspace filename search is available.", vec![]),
        configurable_capability("tool.create-directory", "Create directory", HarnessCapabilityKind::Tool, "Authorized workspace directory creation is available.", vec!["changes.accounting"]),
        configurable_capability("tool.apply-patch", "Apply patch", HarnessCapabilityKind::Tool, "Exact-image multi-file patches apply transactionally with rollback.", vec!["changes.accounting"]),
        configurable_capability("tool.write-text", "Write text", HarnessCapabilityKind::Tool, "Authorized exact-image UTF-8 writes are available.", vec!["changes.accounting"]),
        configurable_capability("tool.edit-text", "Edit guarded text", HarnessCapabilityKind::Tool, "Authorized revision-bound unique text replacement is available with exact matching by default and explicit uniform line-ending preservation.", vec!["changes.accounting"]),
        configurable_capability("tool.move-file", "Move file", HarnessCapabilityKind::Tool, "Authorized file moves are available.", vec!["changes.accounting"]),
        configurable_capability("tool.copy-file", "Copy file", HarnessCapabilityKind::Tool, "Authorized file copies are available.", vec!["changes.accounting"]),
        configurable_capability("tool.delete-file", "Delete file", HarnessCapabilityKind::Tool, "Authorized exact-image file deletion is available.", vec!["changes.accounting"]),
        unavailable_capability(
            "tool.network",
            "Network tool",
            HarnessCapabilityKind::Tool,
            "No model-facing network tool is registered.",
        ),
        configurable_capability(
            "git.read",
            "Git inspection",
            HarnessCapabilityKind::Execution,
            "Fixed read-only Git operations are available.",
            vec![],
        ),
        configurable_capability(
            "git.write",
            "Git mutation",
            HarnessCapabilityKind::Execution,
            "A fixed allowlist of local Git mutation operations is available.",
            vec!["approval.write"],
        ),
        configurable_capability(
            "verification.runner",
            "Verification runners",
            HarnessCapabilityKind::Review,
            "Fixed fmt, check, test, and clippy runners produce durable evidence.",
            vec![],
        ),
        optional_rust_check_capability(),
    ]
}

fn optional_rust_check_capability() -> CapabilityRegistration {
    let mut registration=configurable_capability(
            "verification.supplemental-rust",
            "Isolated supplemental Rust checks",
            HarnessCapabilityKind::Execution,
            "Owner-enabled model-authored Rust assertions in a Windows LPAC byte-copy, not arbitrary host commands or independent proof of task correctness.",
            vec!["verification.runner"],
        );
    registration.enabled = false;
    registration.available = cfg!(windows);
    registration
}

fn available_capability(
    id: &str,
    label: &str,
    kind: HarnessCapabilityKind,
    detail: &str,
    dependencies: Vec<&str>,
) -> CapabilityRegistration {
    CapabilityRegistration {
        id: id.to_owned(),
        label: label.to_owned(),
        kind,
        available: true,
        enabled: true,
        configurable: false,
        readiness: HarnessReadiness::Ready,
        detail: detail.to_owned(),
        unavailable_reason: String::new(),
        dependencies: dependencies.into_iter().map(str::to_owned).collect(),
    }
}

fn configurable_capability(
    id: &str,
    label: &str,
    kind: HarnessCapabilityKind,
    detail: &str,
    dependencies: Vec<&str>,
) -> CapabilityRegistration {
    let mut capability = available_capability(id, label, kind, detail, dependencies);
    capability.configurable = true;
    capability
}

fn disabled_configurable_capability(
    id: &str,
    label: &str,
    kind: HarnessCapabilityKind,
    detail: &str,
    dependencies: Vec<&str>,
) -> CapabilityRegistration {
    let mut capability = configurable_capability(id, label, kind, detail, dependencies);
    capability.enabled = false;
    capability
}

fn external_extension_capability() -> CapabilityRegistration {
    #[cfg(feature = "extensions")]
    {
        disabled_configurable_capability("extension.external", "Reviewed extensions", HarnessCapabilityKind::Extension,
            "Optional pinned skills/MCP adapters require exact owner activation; current-user executable code is not filesystem sandboxed.", vec!["approval.write"])
    }
    #[cfg(not(feature = "extensions"))]
    {
        unavailable_capability(
            "extension.external",
            "External modules",
            HarnessCapabilityKind::Extension,
            "This build omits optional extension support; use an extensions-enabled build.",
        )
    }
}

fn unavailable_capability(
    id: &str,
    label: &str,
    kind: HarnessCapabilityKind,
    reason: &str,
) -> CapabilityRegistration {
    CapabilityRegistration {
        id: id.to_owned(),
        label: label.to_owned(),
        kind,
        available: false,
        enabled: false,
        configurable: false,
        readiness: HarnessReadiness::Unavailable,
        detail: String::new(),
        unavailable_reason: reason.to_owned(),
        dependencies: Vec::new(),
    }
}

fn built_in_surfaces() -> Vec<SurfaceRegistration> {
    vec![
        available_surface(
            SurfaceId::Conversation,
            false,
            "Primary session transcript and composer. It cannot be hidden.",
        ),
        available_surface(
            SurfaceId::Plan,
            false,
            "Runtime plan stages appear inside the conversation when present.",
        ),
        available_surface(
            SurfaceId::Changes,
            true,
            "Current run summary only. Per-file diff review is not implemented.",
        ),
        unavailable_surface(
            SurfaceId::Files,
            "The workspace file browser is not implemented.",
        ),
        unavailable_surface(
            SurfaceId::Terminal,
            "The persistent terminal backend and surface are not implemented.",
        ),
        unavailable_surface(SurfaceId::Preview, "No preview renderer is registered."),
        available_surface(
            SurfaceId::Activity,
            true,
            "Recent factual run, provider, tool, approval, and verification events.",
        ),
        available_surface(
            SurfaceId::Trajectory,
            true,
            "Bounded durable task, lifecycle, tool, approval, usage, change, and verification records for one session.",
        ),
        available_surface(
            SurfaceId::Approvals,
            false,
            "Opens only while an action needs a user decision.",
        ),
    ]
}

fn available_surface(id: SurfaceId, configurable: bool, detail: &str) -> SurfaceRegistration {
    SurfaceRegistration {
        id,
        available: true,
        configurable,
        detail: detail.to_owned(),
        unavailable_reason: String::new(),
    }
}

fn unavailable_surface(id: SurfaceId, reason: &str) -> SurfaceRegistration {
    SurfaceRegistration {
        id,
        available: false,
        configurable: false,
        detail: String::new(),
        unavailable_reason: reason.to_owned(),
    }
}

fn built_in_profiles() -> Vec<ProfileRegistration> {
    vec![
        profile(
            "focused",
            "Focused",
            "Keep secondary work surfaces closed.",
            10,
            false,
            false,
            false,
        ),
        profile(
            DEFAULT_PROFILE_ID,
            "Standard",
            "Default session workspace with optional surfaces closed.",
            20,
            false,
            false,
            false,
        ),
        profile(
            "review",
            "Review",
            "Open the Changes summary beside the active session.",
            30,
            true,
            false,
            false,
        ),
        profile(
            "trace",
            "Trace",
            "Open the durable Trajectory while keeping Changes and live Activity closed.",
            40,
            false,
            false,
            true,
        ),
        profile(
            "full",
            "Full",
            "Open Activity, Trajectory, and the Changes summary.",
            50,
            true,
            true,
            true,
        ),
    ]
}

fn built_in_execution_profiles() -> Vec<ExecutionProfileRegistration> {
    vec![
        execution_profile(
            "observe",
            "Observe",
            "Allow bounded inspection and fixed read-only Git operations.",
            10,
            SandboxScope::ReadOnly,
            ApprovalMode::Always,
            false,
        ),
        execution_profile(
            DEFAULT_EXECUTION_PROFILE_ID,
            "Guided",
            "Require explicit approval for every executable or mutating action.",
            20,
            SandboxScope::WorkspaceWrite,
            ApprovalMode::Always,
            true,
        ),
        execution_profile(
            "supervised",
            "Supervised",
            "Prompt for risky executable or mutating actions.",
            30,
            SandboxScope::WorkspaceWrite,
            ApprovalMode::OnRisk,
            true,
        ),
        execution_profile(
            "automatic",
            "Automatic",
            "Run enabled in-process workspace capabilities without prompts. Host processes still require approval.",
            40,
            SandboxScope::WorkspaceWrite,
            ApprovalMode::Never,
            true,
        ),
        verification_execution_profile(),
        execution_profile(
            "full-access",
            "Full Access",
            "Run enabled capabilities without workspace containment or interactive approval.",
            50,
            SandboxScope::Unrestricted,
            ApprovalMode::Never,
            true,
        ),
        execution_profile(
            "developer",
            "Developer",
            "Enable the complete workspace-scoped toolkit and prompt for risky actions.",
            60,
            SandboxScope::WorkspaceWrite,
            ApprovalMode::OnRisk,
            true,
        ),
    ]
}

fn verification_execution_profile() -> ExecutionProfileRegistration {
    let mut profile = execution_profile(
        "verification",
        "Verification",
        "Owner-selected bounded read, edit, and fixed verification. Arbitrary processes remain approval-gated.",
        45,
        SandboxScope::WorkspaceWrite,
        ApprovalMode::Never,
        true,
    );
    for enabled in profile.capability_enabled.values_mut() {
        *enabled = false;
    }
    for capability_id in ["tool.read-text", "tool.edit-text", "verification.runner"] {
        profile
            .capability_enabled
            .insert(capability_id.to_owned(), true);
    }
    profile
}

fn execution_profile(
    id: &str,
    label: &str,
    detail: &str,
    order: u16,
    sandbox_scope: SandboxScope,
    approval_mode: ApprovalMode,
    execution_enabled: bool,
) -> ExecutionProfileRegistration {
    let read_capabilities = [
        "tool.list-directory",
        "tool.file-metadata",
        "tool.read-text",
        "tool.read-binary-metadata",
        "tool.search-text",
        "tool.search-filenames",
        "git.read",
    ];
    let execution_capabilities = [
        "tool.create-directory",
        "tool.apply-patch",
        "tool.write-text",
        "tool.edit-text",
        "tool.move-file",
        "tool.copy-file",
        "tool.delete-file",
        "execution.argv",
        "execution.shell",
        "git.write",
        "verification.runner",
    ];
    let mut capability_enabled = BTreeMap::new();
    for capability_id in read_capabilities {
        capability_enabled.insert(capability_id.to_owned(), true);
    }
    for capability_id in execution_capabilities {
        capability_enabled.insert(capability_id.to_owned(), execution_enabled);
    }
    capability_enabled.insert("verification.supplemental-rust".into(), false);
    ExecutionProfileRegistration {
        id: id.to_owned(),
        label: label.to_owned(),
        detail: detail.to_owned(),
        order,
        sandbox_scope,
        approval_mode,
        capability_enabled,
    }
}

fn profile(
    id: &str,
    label: &str,
    detail: &str,
    order: u16,
    changes_visible: bool,
    activity_visible: bool,
    trajectory_visible: bool,
) -> ProfileRegistration {
    ProfileRegistration {
        id: id.to_owned(),
        label: label.to_owned(),
        detail: detail.to_owned(),
        order,
        visibility: BTreeMap::from([
            (SurfaceId::Changes, changes_visible),
            (SurfaceId::Activity, activity_visible),
            (SurfaceId::Trajectory, trajectory_visible),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_snapshot_reports_available_and_missing_work() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let snapshot = registry.snapshot(
            &LayoutProfile::default(),
            &HarnessExecutionPolicy::default(),
            &HarnessFacts::default(),
        );

        assert_eq!(snapshot.selected_profile_id, DEFAULT_PROFILE_ID);
        assert!(snapshot.capabilities.iter().any(|capability| {
            capability.id == "tool.write-text"
                && capability.availability == HarnessAvailability::Available
                && capability.enabled
        }));
        for capability_id in ["changes.accounting", "review.diff"] {
            assert!(snapshot.capabilities.iter().any(|capability| {
                capability.id == capability_id
                    && capability.availability == HarnessAvailability::Available
                    && capability.readiness == HarnessReadiness::Ready
                    && capability.enabled
                    && capability.unavailable_reason.is_empty()
            }));
        }
        assert!(snapshot.capabilities.iter().any(|capability| {
            capability.id == "verification.runner"
                && capability.availability == HarnessAvailability::Available
                && capability.readiness == HarnessReadiness::Ready
                && capability.unavailable_reason.is_empty()
        }));
        assert!(snapshot.capabilities.iter().any(|capability| {
            capability.id == "execution.terminal"
                && capability.availability == HarnessAvailability::Unavailable
                && !capability.unavailable_reason.is_empty()
        }));
    }

    #[test]
    fn review_profile_changes_only_configurable_surface_visibility() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let layout = registry
            .apply_profile(&LayoutProfile::default(), "review")
            .expect("review profile");

        assert_eq!(layout.harness_profile_id, "review");
        assert_eq!(layout.visible.get(&SurfaceId::Conversation), Some(&true));
        assert_eq!(layout.visible.get(&SurfaceId::Changes), Some(&true));
        assert_eq!(layout.visible.get(&SurfaceId::Activity), Some(&false));
        assert_eq!(layout.visible.get(&SurfaceId::Trajectory), Some(&false));
    }

    #[test]
    fn trace_and_full_profiles_expose_the_durable_trajectory_deliberately() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let trace = registry
            .apply_profile(&LayoutProfile::default(), "trace")
            .expect("trace profile");
        assert_eq!(trace.visible.get(&SurfaceId::Trajectory), Some(&true));
        assert_eq!(trace.visible.get(&SurfaceId::Activity), Some(&false));
        assert_eq!(trace.visible.get(&SurfaceId::Changes), Some(&false));

        let full = registry
            .apply_profile(&LayoutProfile::default(), "full")
            .expect("full profile");
        assert_eq!(full.visible.get(&SurfaceId::Trajectory), Some(&true));
        assert_eq!(full.visible.get(&SurfaceId::Activity), Some(&true));
        assert_eq!(full.visible.get(&SurfaceId::Changes), Some(&true));
    }

    #[test]
    fn presentation_profiles_choose_one_inspector_mode_and_expected_collapse_state() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let baseline = LayoutProfile::default();

        for (profile_id, collapsed, active) in [
            ("focused", true, None),
            ("standard", true, None),
            ("review", false, Some(SurfaceId::Changes)),
            ("trace", false, Some(SurfaceId::Trajectory)),
            ("full", false, Some(SurfaceId::Changes)),
        ] {
            let layout = registry
                .apply_profile(&baseline, profile_id)
                .expect("presentation profile");
            assert_eq!(layout.inspector_collapsed, collapsed, "{profile_id}");
            assert_eq!(layout.active_inspector, active, "{profile_id}");
        }
    }

    #[test]
    fn inspector_selection_requires_an_enabled_available_inspector_surface() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let mut layout = LayoutProfile::default();
        layout.visible.insert(SurfaceId::Trajectory, true);

        assert_eq!(
            registry
                .set_inspector_surface(&layout, Some(SurfaceId::Plan))
                .expect_err("plan is not an inspector mode"),
            HarnessError::InvalidInspectorSurface("Plan")
        );
        assert_eq!(
            registry
                .set_inspector_surface(&layout, Some(SurfaceId::Activity))
                .expect_err("activity is disabled"),
            HarnessError::InspectorSurfaceDisabled("Activity")
        );

        let selected = registry
            .set_inspector_surface(&layout, Some(SurfaceId::Trajectory))
            .expect("trajectory selected");
        assert_eq!(selected.active_inspector, Some(SurfaceId::Trajectory));
        assert!(!selected.inspector_collapsed);
        assert_eq!(selected.visible, layout.visible);

        let collapsed = registry
            .set_inspector_surface(&selected, None)
            .expect("inspector collapsed");
        assert!(collapsed.inspector_collapsed);
        assert_eq!(collapsed.active_inspector, Some(SurfaceId::Trajectory));
        assert_eq!(collapsed.visible, layout.visible);
    }

    #[test]
    fn inspector_reconciliation_uses_deterministic_enabled_fallback() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let mut layout = LayoutProfile::default();
        layout.visible.insert(SurfaceId::Changes, true);
        layout.visible.insert(SurfaceId::Activity, true);
        layout.active_inspector = Some(SurfaceId::Trajectory);
        layout.inspector_collapsed = false;

        let reconciled = registry.reconcile_layout(&layout);
        assert_eq!(reconciled.active_inspector, Some(SurfaceId::Changes));
        assert!(!reconciled.inspector_collapsed);

        let changes_hidden = registry
            .set_surface_visibility(&reconciled, SurfaceId::Changes, false)
            .expect("hide changes");
        assert_eq!(changes_hidden.active_inspector, Some(SurfaceId::Activity));
        assert!(!changes_hidden.inspector_collapsed);

        let all_hidden = registry
            .set_surface_visibility(&changes_hidden, SurfaceId::Activity, false)
            .expect("hide activity");
        assert_eq!(all_hidden.active_inspector, None);
        assert!(all_hidden.inspector_collapsed);
    }

    #[test]
    fn individual_visibility_becomes_custom_and_unavailable_surfaces_fail() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let layout = registry
            .set_surface_visibility(&LayoutProfile::default(), SurfaceId::Activity, true)
            .expect("activity visible");
        assert_eq!(layout.harness_profile_id, CUSTOM_PROFILE_ID);
        assert_eq!(layout.visible.get(&SurfaceId::Activity), Some(&true));

        assert_eq!(
            registry
                .set_surface_visibility(&layout, SurfaceId::Terminal, true)
                .expect_err("terminal unavailable"),
            HarnessError::UnavailableSurface("Terminal")
        );
        assert_eq!(
            registry
                .set_surface_visibility(&layout, SurfaceId::Approvals, false)
                .expect_err("approval lifecycle controlled"),
            HarnessError::FixedSurface("Approvals")
        );
    }

    #[test]
    fn readiness_facts_do_not_turn_unavailable_capabilities_ready() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let facts = HarnessFacts::default()
            .with_readiness("model.streaming", HarnessReadiness::NeedsConfiguration)
            .with_readiness("execution.terminal", HarnessReadiness::Ready);
        let snapshot = registry.snapshot(
            &LayoutProfile::default(),
            &HarnessExecutionPolicy::default(),
            &facts,
        );

        assert!(snapshot.capabilities.iter().any(|capability| {
            capability.id == "model.streaming"
                && capability.readiness == HarnessReadiness::NeedsConfiguration
        }));
        assert!(snapshot.capabilities.iter().any(|capability| {
            capability.id == "execution.terminal"
                && capability.readiness == HarnessReadiness::Unavailable
        }));
    }

    #[test]
    fn reconcile_layout_closes_unavailable_surfaces_and_detects_custom_state() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let mut layout = LayoutProfile::default();
        layout.visible.insert(SurfaceId::Terminal, true);
        layout.visible.insert(SurfaceId::Activity, true);
        let layout = registry.reconcile_layout(&layout);

        assert_eq!(layout.visible.get(&SurfaceId::Terminal), Some(&false));
        assert_eq!(layout.visible.get(&SurfaceId::Conversation), Some(&true));
        assert_eq!(layout.harness_profile_id, CUSTOM_PROFILE_ID);
    }

    #[test]
    fn registry_rejects_missing_dependencies_and_cycles() {
        let mut missing = HarnessRegistry::default();
        missing
            .register_capability(available_capability(
                "test.one",
                "Test one",
                HarnessCapabilityKind::Agent,
                "Test capability.",
                vec!["test.missing"],
            ))
            .expect("registered");
        assert!(matches!(
            missing.validate(),
            Err(HarnessError::MissingDependency { .. })
        ));

        let mut cyclic = HarnessRegistry::default();
        cyclic
            .register_capability(available_capability(
                "test.one",
                "Test one",
                HarnessCapabilityKind::Agent,
                "Test capability.",
                vec!["test.two"],
            ))
            .expect("first");
        cyclic
            .register_capability(available_capability(
                "test.two",
                "Test two",
                HarnessCapabilityKind::Agent,
                "Test capability.",
                vec!["test.one"],
            ))
            .expect("second");
        assert!(matches!(
            cyclic.validate(),
            Err(HarnessError::DependencyCycle(_))
        ));
    }

    #[test]
    fn duplicate_registration_is_rejected_without_replacing_the_original() {
        let mut registry = HarnessRegistry::default();
        registry
            .register_capability(available_capability(
                "test.one",
                "Original",
                HarnessCapabilityKind::Agent,
                "Original registration.",
                vec![],
            ))
            .expect("original");
        let error = registry
            .register_capability(available_capability(
                "test.one",
                "Replacement",
                HarnessCapabilityKind::Agent,
                "Replacement registration.",
                vec![],
            ))
            .expect_err("duplicate");
        assert_eq!(
            error,
            HarnessError::DuplicateRegistration("test.one".into())
        );
        assert_eq!(
            registry
                .capabilities
                .get("test.one")
                .expect("original remains")
                .label,
            "Original"
        );
    }

    #[test]
    fn execution_profiles_resolve_effective_text_tool_enablement() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let default_policy = HarnessExecutionPolicy::default();
        let default_snapshot = registry.snapshot(
            &LayoutProfile::default(),
            &default_policy,
            &HarnessFacts::default(),
        );
        assert_eq!(
            default_snapshot.selected_execution_profile_id,
            DEFAULT_EXECUTION_PROFILE_ID
        );
        assert!(registry.capability_is_enabled(&default_policy, "tool.read-text"));
        assert!(registry.capability_is_enabled(&default_policy, "tool.write-text"));

        let read_only = registry
            .apply_execution_profile(&default_policy, "read-only")
            .expect("read-only profile");
        assert_eq!(read_only.profile_id, "observe");
        assert!(registry.capability_is_enabled(&read_only, "tool.read-text"));
        assert!(!registry.capability_is_enabled(&read_only, "tool.write-text"));
        let snapshot = registry.snapshot(
            &LayoutProfile::default(),
            &read_only,
            &HarnessFacts::default(),
        );
        assert!(snapshot.capabilities.iter().any(|capability| {
            capability.id == "tool.write-text"
                && capability.availability == HarnessAvailability::Available
                && capability.configurable
                && !capability.enabled
        }));
    }

    #[test]
    fn individual_tool_enablement_becomes_custom_and_fixed_entries_fail() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let policy = registry
            .set_capability_enabled(&HarnessExecutionPolicy::default(), "tool.write-text", false)
            .expect("disable writes");
        assert_eq!(policy.profile_id, CUSTOM_EXECUTION_PROFILE_ID);
        assert_eq!(
            policy.source_profile_id.as_deref(),
            Some(DEFAULT_EXECUTION_PROFILE_ID)
        );
        assert!(!registry.capability_is_enabled(&policy, "tool.write-text"));

        assert_eq!(
            registry
                .set_capability_enabled(&policy, "approval.write", false)
                .expect_err("approval is fixed"),
            HarnessError::FixedCapability("approval.write".into())
        );
        let policy = registry
            .set_capability_enabled(&policy, "verification.runner", false)
            .expect("verification is configurable");
        assert!(!registry.capability_is_enabled(&policy, "verification.runner"));
    }

    #[test]
    fn fixed_verification_is_independent_of_generic_argv_enablement() {
        let registry = HarnessRegistry::built_in().expect("registry");
        for profile_id in ["verification", "developer"] {
            let policy = registry
                .apply_execution_profile(&HarnessExecutionPolicy::default(), profile_id)
                .expect("profile");
            let policy = registry
                .set_capability_enabled(&policy, "execution.argv", false)
                .expect("disable arbitrary commands without disabling verification");
            assert!(registry.capability_is_enabled(&policy, "verification.runner"));
            assert!(!registry.capability_is_enabled(&policy, "execution.argv"));
            assert_eq!(policy.source_profile_id.as_deref(), Some(profile_id));
            let restored = registry.reconcile_execution_policy(&policy);
            assert_eq!(restored, policy);
            let disabled = registry
                .set_capability_enabled(&restored, "verification.runner", false)
                .expect("disable verification");
            let enabled = registry
                .set_capability_enabled(&disabled, "verification.runner", true)
                .expect("enable fixed verification without generic commands");
            assert!(registry.capability_is_enabled(&enabled, "verification.runner"));
            assert!(!registry.capability_is_enabled(&enabled, "execution.argv"));
        }
    }

    #[test]
    fn malformed_execution_policy_is_bounded_to_registered_configurable_capabilities() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let mut policy = HarnessExecutionPolicy {
            profile_id: "unknown".into(),
            source_profile_id: Some("unknown".into()),
            sandbox_scope: SandboxScope::WorkspaceWrite,
            approval_mode: ApprovalMode::OnRisk,
            capability_enabled: BTreeMap::from([
                ("tool.read-text".into(), false),
                ("unknown.capability".into(), true),
                ("verification.runner".into(), true),
            ]),
        };
        policy = registry.reconcile_execution_policy(&policy);

        assert_eq!(policy.profile_id, CUSTOM_EXECUTION_PROFILE_ID);
        assert_eq!(policy.capability_enabled.len(), 20);
        assert_eq!(
            policy
                .capability_enabled
                .get("verification.supplemental-rust"),
            Some(&false)
        );
        assert_eq!(
            policy.capability_enabled.get("tool.read-text"),
            Some(&false)
        );
        assert_eq!(
            policy.capability_enabled.get("tool.write-text"),
            Some(&true)
        );
        assert!(!policy.capability_enabled.contains_key("unknown.capability"));
        assert!(policy
            .capability_enabled
            .contains_key("verification.runner"));
    }

    #[test]
    fn phase4_profiles_publish_exact_sandbox_approval_and_capability_matrix() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let baseline = HarnessExecutionPolicy::default();
        let cases = [
            (
                "observe",
                argentum_domain::SandboxScope::ReadOnly,
                argentum_domain::ApprovalMode::Always,
                false,
                false,
            ),
            (
                "guided",
                argentum_domain::SandboxScope::WorkspaceWrite,
                argentum_domain::ApprovalMode::Always,
                true,
                true,
            ),
            (
                "supervised",
                argentum_domain::SandboxScope::WorkspaceWrite,
                argentum_domain::ApprovalMode::OnRisk,
                true,
                true,
            ),
            (
                "automatic",
                argentum_domain::SandboxScope::WorkspaceWrite,
                argentum_domain::ApprovalMode::Never,
                true,
                true,
            ),
            (
                "verification",
                argentum_domain::SandboxScope::WorkspaceWrite,
                argentum_domain::ApprovalMode::Never,
                false,
                false,
            ),
            (
                "full-access",
                argentum_domain::SandboxScope::Unrestricted,
                argentum_domain::ApprovalMode::Never,
                true,
                true,
            ),
            (
                "developer",
                argentum_domain::SandboxScope::WorkspaceWrite,
                argentum_domain::ApprovalMode::OnRisk,
                true,
                true,
            ),
        ];
        for (profile_id, sandbox_scope, approval_mode, write_enabled, argv_enabled) in cases {
            let policy = registry
                .apply_execution_profile(&baseline, profile_id)
                .expect("execution profile");
            assert_eq!(policy.profile_id, profile_id);
            assert_eq!(policy.sandbox_scope, sandbox_scope, "{profile_id}");
            assert_eq!(policy.approval_mode, approval_mode, "{profile_id}");
            assert_eq!(
                registry.capability_is_enabled(&policy, "tool.write-text"),
                write_enabled,
                "{profile_id}"
            );
            assert_eq!(
                registry.capability_is_enabled(&policy, "execution.argv"),
                argv_enabled,
                "{profile_id}"
            );
            assert!(
                !registry.capability_is_enabled(&policy, "model.delegate-readonly"),
                "{profile_id}"
            );
        }
    }

    #[test]
    fn phase4_harness_reports_every_tool_family_ready_but_network_and_pty_unavailable() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let policy = registry
            .apply_execution_profile(&HarnessExecutionPolicy::default(), "developer")
            .expect("developer profile");
        let snapshot =
            registry.snapshot(&LayoutProfile::default(), &policy, &HarnessFacts::default());
        for capability_id in [
            "tool.list-directory",
            "tool.file-metadata",
            "tool.read-text",
            "tool.read-binary-metadata",
            "tool.search-text",
            "tool.search-filenames",
            "tool.create-directory",
            "tool.apply-patch",
            "tool.write-text",
            "tool.move-file",
            "tool.copy-file",
            "tool.delete-file",
            "execution.argv",
            "execution.shell",
            "git.read",
            "git.write",
            "verification.runner",
        ] {
            assert!(
                snapshot.capabilities.iter().any(|capability| {
                    capability.id == capability_id
                        && capability.availability == HarnessAvailability::Available
                        && capability.readiness == HarnessReadiness::Ready
                        && capability.enabled
                }),
                "missing ready capability {capability_id}"
            );
        }
        for capability_id in ["tool.network", "execution.terminal"] {
            assert!(snapshot.capabilities.iter().any(|capability| {
                capability.id == capability_id
                    && capability.availability == HarnessAvailability::Unavailable
                    && !capability.enabled
            }));
        }
    }

    #[test]
    fn phase4_legacy_profiles_reconcile_to_current_profiles_without_unrestricted_custom_state() {
        let registry = HarnessRegistry::built_in().expect("registry");
        let read_only = registry.reconcile_execution_policy(&HarnessExecutionPolicy {
            profile_id: "read-only".into(),
            source_profile_id: Some("read-only".into()),
            sandbox_scope: argentum_domain::SandboxScope::WorkspaceWrite,
            approval_mode: argentum_domain::ApprovalMode::Always,
            capability_enabled: BTreeMap::from([
                ("tool.read-text".into(), true),
                ("tool.write-text".into(), false),
            ]),
        });
        assert_eq!(read_only.profile_id, "observe");
        assert_eq!(
            read_only.sandbox_scope,
            argentum_domain::SandboxScope::ReadOnly
        );

        let guided = registry.reconcile_execution_policy(&HarnessExecutionPolicy {
            profile_id: "confirm-before-changes".into(),
            ..HarnessExecutionPolicy::default()
        });
        assert_eq!(guided.profile_id, "guided");

        let malformed = registry.reconcile_execution_policy(&HarnessExecutionPolicy {
            profile_id: "custom".into(),
            source_profile_id: None,
            sandbox_scope: argentum_domain::SandboxScope::Unrestricted,
            approval_mode: argentum_domain::ApprovalMode::Never,
            capability_enabled: BTreeMap::new(),
        });
        assert_eq!(malformed.profile_id, CUSTOM_EXECUTION_PROFILE_ID);
        assert_eq!(
            malformed.sandbox_scope,
            argentum_domain::SandboxScope::WorkspaceWrite
        );
        assert_eq!(
            malformed.approval_mode,
            argentum_domain::ApprovalMode::OnRisk
        );
    }
}
