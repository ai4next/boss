use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use boss_api::{RuntimeCapabilities, RuntimeProviderStatus};
use parking_lot::RwLock;

use crate::runtime_trait::Runtime;
use crate::spec::{RuntimeClass, SandboxClass};

/// Dispatches sandbox operations to registered runtime providers.
#[derive(Clone)]
pub struct RuntimeManager {
    providers: Arc<RwLock<HashMap<String, Arc<dyn Runtime>>>>,
    class_index: Arc<RwLock<BTreeMap<SandboxClass, Vec<String>>>>,
}

impl Default for RuntimeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            class_index: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Register a provider for a runtime class.
    pub fn register(&self, class: RuntimeClass, provider: Arc<dyn Runtime>) {
        self.register_provider(provider, vec![class.as_str().to_string()]);
    }

    pub fn register_provider(&self, provider: Arc<dyn Runtime>, classes: Vec<SandboxClass>) {
        let name = provider.name().to_string();
        self.providers.write().insert(name.clone(), provider);
        let mut class_index = self.class_index.write();
        for class in classes {
            let providers = class_index.entry(class).or_default();
            if !providers.iter().any(|candidate| candidate == &name) {
                providers.push(name.clone());
            }
        }
    }

    pub fn has(&self, class: RuntimeClass) -> bool {
        self.default_provider_for_class(class.as_str()).is_some()
    }

    /// Look up the provider for a class.
    pub fn provider(&self, class: RuntimeClass) -> Option<Arc<dyn Runtime>> {
        self.default_provider_for_class(class.as_str())
    }

    pub fn provider_by_name(&self, name: &str) -> Option<Arc<dyn Runtime>> {
        self.providers.read().get(name).cloned()
    }

    pub fn providers_for_class(&self, class: &str) -> Vec<Arc<dyn Runtime>> {
        let names = self
            .class_index
            .read()
            .get(class)
            .cloned()
            .unwrap_or_default();
        let providers = self.providers.read();
        names
            .into_iter()
            .filter_map(|name| providers.get(&name).cloned())
            .collect()
    }

    pub fn default_provider_for_class(&self, class: &str) -> Option<Arc<dyn Runtime>> {
        self.providers_for_class(class).into_iter().next()
    }

    pub async fn all_capabilities(&self) -> RuntimeCapabilities {
        let providers: Vec<Arc<dyn Runtime>> = self.providers.read().values().cloned().collect();
        let mut statuses = Vec::<RuntimeProviderStatus>::new();
        for provider in providers {
            statuses.push(provider.capabilities().await.provider);
        }
        statuses.sort_by(|a, b| a.name.cmp(&b.name));
        RuntimeCapabilities {
            providers: statuses,
        }
    }
}
