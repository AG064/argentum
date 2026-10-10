//! Resolve enabled service providers before publishing lifecycle registrations.
//! This is a composition description, not authority or executable package loading.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

const MAX_MODULES: usize = 128;
const MAX_SERVICES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceContract {
    pub id: String,
    /// Exact interface version, independent of a package's release version.
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRequirement {
    pub contract: ServiceContract,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceModule {
    pub id: String,
    pub provides: Vec<ServiceContract>,
    pub requires: Vec<ServiceRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServiceCompositionError {
    #[error("invalid or duplicate module or service declaration")]
    InvalidDeclaration,
    #[error("composition exceeds its module or service limit")]
    LimitExceeded,
    #[error("module is not installed or enabled: {0}")]
    UnknownModule(String),
    #[error("service requires an explicit compatible provider: {0}")]
    AmbiguousService(String),
    #[error("required service is unavailable: {0}")]
    MissingService(String),
    #[error("selected provider is disabled, missing, or does not offer service: {0}")]
    InvalidSelection(String),
    #[error("service interface version is incompatible: {0}")]
    IncompatibleService(String),
    #[error("module dependency cycle")]
    DependencyCycle,
}

#[derive(Debug, Clone)]
pub struct ServiceCatalog {
    modules: BTreeMap<String, ServiceModule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedComposition {
    order: Vec<String>,
    dependencies: BTreeMap<String, Vec<String>>,
    providers: BTreeMap<String, String>,
}

impl ServiceCatalog {
    pub fn new(modules: Vec<ServiceModule>) -> Result<Self, ServiceCompositionError> {
        if modules.len() > MAX_MODULES {
            return Err(ServiceCompositionError::LimitExceeded);
        }
        let mut catalog = BTreeMap::new();
        for module in modules {
            if !valid_id(&module.id) || catalog.contains_key(&module.id) {
                return Err(ServiceCompositionError::InvalidDeclaration);
            }
            if module.provides.len() > MAX_SERVICES || module.requires.len() > MAX_SERVICES {
                return Err(ServiceCompositionError::LimitExceeded);
            }
            validate_contracts(module.provides.iter())?;
            validate_contracts(module.requires.iter().map(|r| &r.contract))?;
            catalog.insert(module.id.clone(), module);
        }
        Ok(Self { modules: catalog })
    }

    /// Resolve only explicitly enabled modules. Optional requirements never
    /// install or enable a provider. Every selected binding must be valid.
    pub fn resolve(
        &self,
        enabled: &BTreeSet<String>,
        selections: &BTreeMap<String, String>,
    ) -> Result<ResolvedComposition, ServiceCompositionError> {
        if enabled.len() > MAX_MODULES || selections.len() > MAX_MODULES * MAX_SERVICES {
            return Err(ServiceCompositionError::LimitExceeded);
        }
        if enabled.iter().any(|id| !valid_id(id))
            || selections
                .iter()
                .any(|(service, provider)| !valid_id(service) || !valid_id(provider))
        {
            return Err(ServiceCompositionError::InvalidDeclaration);
        }
        let mut offers: BTreeMap<String, Vec<(&str, u32)>> = BTreeMap::new();
        let mut requirements: BTreeMap<String, Vec<&ServiceRequirement>> = BTreeMap::new();
        for id in enabled {
            let module = self
                .modules
                .get(id)
                .ok_or_else(|| ServiceCompositionError::UnknownModule(id.clone()))?;
            for offer in &module.provides {
                offers
                    .entry(offer.id.clone())
                    .or_default()
                    .push((&module.id, offer.version));
            }
            for requirement in &module.requires {
                requirements
                    .entry(requirement.contract.id.clone())
                    .or_default()
                    .push(requirement);
            }
        }
        for (service, provider) in selections {
            if !valid_id(service)
                || !valid_id(provider)
                || !offers
                    .get(service)
                    .is_some_and(|items| items.iter().any(|(id, _)| *id == provider))
            {
                return Err(ServiceCompositionError::InvalidSelection(
                    service.chars().take(64).collect(),
                ));
            }
        }
        let mut providers = BTreeMap::new();
        for (service, needs) in &requirements {
            let available = offers.get(service).map(Vec::as_slice).unwrap_or_default();
            if available.is_empty() {
                if needs.iter().any(|need| !need.optional) {
                    return Err(ServiceCompositionError::MissingService(service.clone()));
                }
                continue;
            }
            let compatible: Vec<_> = available
                .iter()
                .filter(|(_, version)| needs.iter().all(|r| r.contract.version == *version))
                .collect();
            let provider = if let Some(selected) = selections.get(service) {
                compatible
                    .iter()
                    .find(|(id, _)| *id == selected)
                    .map(|(id, _)| *id)
                    .ok_or_else(|| ServiceCompositionError::IncompatibleService(service.clone()))?
            } else {
                match compatible.as_slice() {
                    [] => {
                        return Err(ServiceCompositionError::IncompatibleService(
                            service.clone(),
                        ))
                    }
                    [(id, _)] => *id,
                    _ => return Err(ServiceCompositionError::AmbiguousService(service.clone())),
                }
            };
            providers.insert(service.clone(), provider.to_owned());
        }
        // Preserve valid explicit selections even when there is no consumer yet.
        for (service, provider) in selections {
            providers
                .entry(service.clone())
                .or_insert_with(|| provider.clone());
        }
        let mut dependencies = BTreeMap::new();
        for id in enabled {
            let module = &self.modules[id];
            let owners: BTreeSet<_> = module
                .requires
                .iter()
                .filter_map(|r| providers.get(&r.contract.id))
                .filter(|owner| *owner != id)
                .cloned()
                .collect();
            if owners.len() > MAX_SERVICES {
                return Err(ServiceCompositionError::LimitExceeded);
            }
            dependencies.insert(id.clone(), owners.into_iter().collect::<Vec<_>>());
        }
        let mut order = Vec::new();
        let mut remaining = enabled.clone();
        while !remaining.is_empty() {
            let next = remaining
                .iter()
                .find(|id| dependencies[*id].iter().all(|dep| !remaining.contains(dep)))
                .cloned()
                .ok_or(ServiceCompositionError::DependencyCycle)?;
            remaining.remove(&next);
            order.push(next);
        }
        Ok(ResolvedComposition {
            order,
            dependencies,
            providers,
        })
    }
}

impl ResolvedComposition {
    pub fn mount_order(&self) -> &[String] {
        &self.order
    }

    pub fn provider(&self, service: &str) -> Option<&str> {
        self.providers.get(service).map(String::as_str)
    }

    /// Feed these owners into HarnessModuleRegistration.dependencies.
    pub fn module_dependencies(&self, module: &str) -> Option<&[String]> {
        self.dependencies.get(module).map(Vec::as_slice)
    }

    /// Dependents first, root last. Does not perform teardown or data deletion.
    pub fn removal_order(&self, root: &str) -> Result<Vec<String>, ServiceCompositionError> {
        if !valid_id(root) {
            return Err(ServiceCompositionError::InvalidDeclaration);
        }
        if !self.dependencies.contains_key(root) {
            return Err(ServiceCompositionError::UnknownModule(
                root.chars().take(64).collect(),
            ));
        }
        let mut removed = BTreeSet::from([root.to_owned()]);
        for module in &self.order {
            if self.dependencies[module]
                .iter()
                .any(|dep| removed.contains(dep))
            {
                removed.insert(module.clone());
            }
        }
        Ok(self
            .order
            .iter()
            .rev()
            .filter(|id| removed.contains(*id))
            .cloned()
            .collect())
    }
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
}

fn validate_contracts<'a>(
    contracts: impl Iterator<Item = &'a ServiceContract>,
) -> Result<(), ServiceCompositionError> {
    let mut seen = BTreeSet::new();
    for contract in contracts {
        if !valid_id(&contract.id) || contract.version == 0 || !seen.insert(&contract.id) {
            return Err(ServiceCompositionError::InvalidDeclaration);
        }
    }
    Ok(())
}
